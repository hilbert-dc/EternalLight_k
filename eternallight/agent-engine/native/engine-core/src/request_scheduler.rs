use crate::Result;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

pub(crate) fn policy(input: &Value) -> Result<Value> {
    if !input.is_null() && !input.is_object() {
        return Err("resourcePolicy must be an object".into());
    }
    let integer = |key: &str, default: u64, max: u64| -> Result<u64> {
        match input.get(key) {
            None => Ok(default),
            Some(value) => value
                .as_u64()
                .filter(|n| *n > 0 && *n <= max)
                .ok_or_else(|| format!("resourcePolicy.{key} must be an integer from 1 to {max}")),
        }
    };
    let global = integer("maxConcurrentRequests", 4, 64)?;
    let provider = integer("maxRequestsPerProvider", 2, 64)?;
    let queued = integer("maxQueuedRequests", 32, 64)?;
    let overrides = input
        .get("connectionLimits")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let entries = overrides
        .as_object()
        .ok_or("resourcePolicy.connectionLimits must be an object")?;
    if entries.len() > 64
        || entries
            .iter()
            .any(|(id, value)| id.is_empty() || value.as_u64().is_none_or(|v| v == 0 || v > 64))
    {
        return Err("invalid resourcePolicy connection limit".into());
    }
    if let Some(object) = input.as_object() {
        for key in object.keys() {
            if ![
                "maxConcurrentRequests",
                "maxRequestsPerProvider",
                "maxQueuedRequests",
                "connectionLimits",
            ]
            .contains(&key.as_str())
            {
                return Err(format!("unknown resourcePolicy field: {key}"));
            }
        }
    }
    Ok(
        json!({"maxConcurrentRequests":global,"maxRequestsPerProvider":provider,"maxQueuedRequests":queued,"connectionLimits":overrides}),
    )
}

#[derive(Default)]
struct Provider {
    active: usize,
    limit: usize,
    ceiling: usize,
    successes: usize,
}

/// Bounded request admission, shared by all agents of one execution bridge.
pub(crate) struct Scheduler {
    capacity: usize,
    queued_limit: usize,
    pending: VecDeque<(u64, String)>,
    active: HashMap<u64, String>,
    providers: HashMap<String, Provider>,
}

impl Scheduler {
    pub fn new(policy: &Value) -> Self {
        Self {
            capacity: policy["maxConcurrentRequests"].as_u64().unwrap() as usize,
            queued_limit: policy["maxQueuedRequests"].as_u64().unwrap() as usize,
            pending: VecDeque::new(),
            active: HashMap::new(),
            providers: HashMap::new(),
        }
    }
    pub fn enqueue(&mut self, id: u64, provider: String, limit: usize) -> Result<()> {
        if self.pending.len() >= self.queued_limit {
            return Err("model request queue is full".into());
        }
        self.providers.entry(provider.clone()).or_insert(Provider {
            limit,
            ceiling: limit,
            ..Provider::default()
        });
        self.pending.push_back((id, provider));
        Ok(())
    }
    pub fn admit(&mut self, id: u64) -> bool {
        if self.active.len() >= self.capacity {
            return false;
        }
        // A saturated provider cannot block eligible requests for other providers.
        let next = self.pending.iter().position(|(_, group)| {
            let provider = &self.providers[group];
            provider.active < provider.limit
        });
        let Some(index) = next else {
            return false;
        };
        if self.pending[index].0 != id {
            return false;
        }
        let (_, group) = self.pending.remove(index).unwrap();
        self.providers.get_mut(&group).unwrap().active += 1;
        self.active.insert(id, group);
        true
    }
    pub fn feedback(&mut self, id: u64, status: u16) {
        let Some(group) = self.active.get(&id) else {
            return;
        };
        let provider = self.providers.get_mut(group).unwrap();
        if status == 429 {
            provider.limit = (provider.limit / 2).max(1);
            provider.successes = 0;
        } else if (200..300).contains(&status) {
            provider.successes += 1;
            if provider.successes >= 4 {
                provider.limit = (provider.limit + 1).min(provider.ceiling);
                provider.successes = 0;
            }
        } else {
            provider.successes = 0;
        }
    }
    pub fn release(&mut self, id: u64) {
        self.pending.retain(|(queued, _)| *queued != id);
        if let Some(group) = self.active.remove(&id) {
            self.providers.get_mut(&group).unwrap().active -= 1;
        }
    }
    pub fn snapshot(&self) -> Value {
        json!({"running":self.active.len(),"queued":self.pending.len(),"capacity":self.capacity})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queues_release_and_isolate_provider_limits() {
        let mut s = Scheduler::new(&policy(&json!({"maxConcurrentRequests":2})).unwrap());
        s.enqueue(1, "a".into(), 1).unwrap();
        s.enqueue(2, "a".into(), 1).unwrap();
        s.enqueue(3, "b".into(), 1).unwrap();
        assert!(s.admit(1));
        assert!(!s.admit(2));
        assert!(s.admit(3));
        s.release(1);
        assert!(s.admit(2));
        s.release(2);
        s.release(3);
        assert_eq!(s.snapshot()["running"], 0);
    }
    #[test]
    fn backpressure_reduces_and_recovers_capacity() {
        let mut s = Scheduler::new(&policy(&Value::Null).unwrap());
        s.enqueue(1, "a".into(), 4).unwrap();
        assert!(s.admit(1));
        s.feedback(1, 429);
        assert_eq!(s.providers["a"].limit, 2);
        for _ in 0..4 {
            s.feedback(1, 200);
        }
        assert_eq!(s.providers["a"].limit, 3);
        s.release(1);
    }
    #[test]
    fn cancelled_queue_entries_and_invalid_policies() {
        let mut s = Scheduler::new(&policy(&json!({"maxQueuedRequests":1})).unwrap());
        s.enqueue(1, "a".into(), 1).unwrap();
        assert!(s.enqueue(2, "a".into(), 1).is_err());
        s.release(1);
        s.enqueue(2, "a".into(), 1).unwrap();
        assert!(s.admit(2));
        for value in [
            json!({"maxConcurrentRequests":0}),
            json!({"maxQueuedRequests":65}),
            json!({"connectionLimits":{"a":-1}}),
            json!({"typo":2}),
        ] {
            assert!(policy(&value).is_err());
        }
    }
}
