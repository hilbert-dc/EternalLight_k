use crate::{
    Result,
    evidence::{RecordMode, normalize},
    storage,
};
use fs2::FileExt;
use serde_json::{Value, json};
use std::path::Path;

pub fn execute(path: &Path, method: &str, input: &Value) -> Result<Value> {
    let mut lock_path = path.as_os_str().to_owned();
    lock_path.push(".lock");
    let lock = storage::lock_file(Path::new(&lock_path))?;
    FileExt::lock_exclusive(&lock).map_err(|e| e.to_string())?;
    let mut records = Vec::new();
    if method != "replace" {
        let document =
            storage::read_json(path)?.unwrap_or_else(|| json!({"version":1,"records":[]}));
        if document["version"] != 1 || !document["records"].is_array() {
            return Err("unsupported evidence store".into());
        }
        records = document["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| normalize(r, RecordMode::Bound))
            .collect::<Result<_>>()?;
    }
    match method {
        "read" => {}
        "append" | "replace" => {
            let entries = input
                .as_array()
                .ok_or("evidence records must be an array")?;
            records.extend(
                entries
                    .iter()
                    .map(|r| normalize(r, RecordMode::Bound))
                    .collect::<Result<Vec<_>>>()?,
            );
            storage::durable_replace_json(path, &json!({"version":1,"records":records}))?;
        }
        _ => return Err("unknown evidence store operation".into()),
    }
    Ok(records.into())
}
