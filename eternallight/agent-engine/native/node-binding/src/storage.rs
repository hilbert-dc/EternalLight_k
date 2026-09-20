use napi::{Env, Task, bindgen_prelude::AsyncTask};
use napi_derive::napi;
use serde_json::Value;
use std::path::PathBuf;

enum Operation {
    Recovery { method: String, input: Value },
    Evidence { method: String, input: Value },
    Report { method: String, input: Value },
    Replace(Value),
}

pub struct StorageTask {
    path: PathBuf,
    operation: Operation,
}

impl Task for StorageTask {
    type Output = String;
    type JsValue = String;
    fn compute(&mut self) -> napi::Result<String> {
        let value = match &self.operation {
            Operation::Recovery { method, input } => {
                eternallight_engine_core::recovery_operation(&self.path, method, input)
            }
            Operation::Evidence { method, input } => {
                eternallight_engine_core::evidence_store_operation(&self.path, method, input)
            }
            Operation::Report { method, input } => {
                eternallight_engine_core::report_store_operation(&self.path, method, input)
            }
            Operation::Replace(document) => {
                eternallight_engine_core::durable_replace_json(&self.path, document)
                    .map(|()| Value::Null)
            }
        }
        .map_err(napi::Error::from_reason)?;
        serde_json::to_string(&value)
            .map_err(|_| napi::Error::from_reason("invalid storage result"))
    }
    fn resolve(&mut self, _env: Env, output: String) -> napi::Result<String> {
        Ok(output)
    }
}

fn parse(input: &str) -> napi::Result<Value> {
    if input.len() > 32 * 1024 * 1024 {
        return Err(napi::Error::from_reason("storage input exceeds limit"));
    }
    serde_json::from_str(input).map_err(|_| napi::Error::from_reason("invalid storage input"))
}

#[napi]
pub struct NativeRecoveryIndex {
    path: PathBuf,
}

#[napi]
impl NativeRecoveryIndex {
    #[napi(constructor)]
    pub fn new(path: String) -> napi::Result<Self> {
        if path.is_empty() {
            return Err(napi::Error::from_reason("recovery path is required"));
        }
        Ok(Self { path: path.into() })
    }

    #[napi]
    pub fn apply(&self, method: String, input: String) -> napi::Result<AsyncTask<StorageTask>> {
        Ok(AsyncTask::new(StorageTask {
            path: self.path.clone(),
            operation: Operation::Recovery {
                method,
                input: parse(&input)?,
            },
        }))
    }
}

#[napi]
pub fn durable_replace_json(
    path: String,
    document: String,
) -> napi::Result<AsyncTask<StorageTask>> {
    if path.is_empty() {
        return Err(napi::Error::from_reason("storage path is required"));
    }
    Ok(AsyncTask::new(StorageTask {
        path: path.into(),
        operation: Operation::Replace(parse(&document)?),
    }))
}

#[napi]
pub struct NativeEvidenceStore {
    path: PathBuf,
}

#[napi]
pub struct NativeReportStore {
    path: PathBuf,
}

#[napi]
impl NativeReportStore {
    #[napi(constructor)]
    pub fn new(path: String) -> napi::Result<Self> {
        if path.is_empty() {
            return Err(napi::Error::from_reason("report directory is required"));
        }
        Ok(Self { path: path.into() })
    }

    #[napi]
    pub fn apply(&self, method: String, input: String) -> napi::Result<AsyncTask<StorageTask>> {
        Ok(AsyncTask::new(StorageTask {
            path: self.path.clone(),
            operation: Operation::Report {
                method,
                input: parse(&input)?,
            },
        }))
    }
}

#[napi]
impl NativeEvidenceStore {
    #[napi(constructor)]
    pub fn new(path: String) -> napi::Result<Self> {
        if path.is_empty() {
            return Err(napi::Error::from_reason("evidence path is required"));
        }
        Ok(Self { path: path.into() })
    }

    #[napi]
    pub fn apply(&self, method: String, input: String) -> napi::Result<AsyncTask<StorageTask>> {
        Ok(AsyncTask::new(StorageTask {
            path: self.path.clone(),
            operation: Operation::Evidence {
                method,
                input: parse(&input)?,
            },
        }))
    }
}
