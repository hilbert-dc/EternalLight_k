use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::RepositoryError;

#[derive(Clone)]
struct Source {
    object: Value,
    revision: Value,
    path: Value,
}

pub(crate) fn normalize(
    value: &mut Value,
    evidence: &mut BTreeMap<String, Value>,
) -> Result<(), RepositoryError> {
    normalize_node(value, /*inherited*/ None, evidence)
}

fn normalize_node(
    value: &mut Value,
    inherited: Option<&Source>,
    evidence: &mut BTreeMap<String, Value>,
) -> Result<(), RepositoryError> {
    match value {
        Value::Object(object) => {
            let source = if object.contains_key("sourceObjectId") {
                let source = Source {
                    object: object["sourceObjectId"].clone(),
                    revision: object
                        .get("sourceRevisionId")
                        .cloned()
                        .unwrap_or(Value::Null),
                    path: object.get("sourcePath").cloned().unwrap_or(json!("/body")),
                };
                if [&source.object, &source.revision, &source.path]
                    .iter()
                    .any(|field| field.as_str().is_none_or(str::is_empty))
                {
                    return Err(RepositoryError::CorruptState);
                }
                Some(source)
            } else {
                inherited.cloned()
            };
            for key in ["evidence", "epistemicUntilEvidence"] {
                if let Some(anchor) = object.remove(key) {
                    if anchor.is_null() {
                        continue;
                    }
                    let source = source.as_ref().ok_or(RepositoryError::CorruptState)?;
                    let quote = anchor["quote"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?;
                    let start = anchor["start"]
                        .as_u64()
                        .ok_or(RepositoryError::CorruptState)?;
                    let end = anchor["end"]
                        .as_u64()
                        .ok_or(RepositoryError::CorruptState)?;
                    if quote.is_empty()
                        || end.checked_sub(start) != Some(quote.chars().count() as u64)
                    {
                        return Err(RepositoryError::CorruptState);
                    }
                    let citation = json!({"sourceObjectId":source.object,"sourceRevisionId":source.revision,
                        "sourcePath":source.path,"quote":quote,"start":start,"end":end});
                    let id = format!("{:x}", Sha256::digest(citation.to_string().as_bytes()));
                    evidence.insert(id.clone(), citation);
                    object.insert(format!("{key}Ref"), json!(id));
                }
            }
            for child in object.values_mut() {
                normalize_node(child, source.as_ref(), evidence)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                normalize_node(child, inherited, evidence)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

pub(crate) fn retain_referenced(report: &mut Value) -> Result<(), RepositoryError> {
    let mut referenced = BTreeSet::new();
    collect_refs(&report["steps"], &mut referenced)?;
    let dictionary = report["evidence"]
        .as_object_mut()
        .ok_or(RepositoryError::CorruptState)?;
    if referenced.iter().any(|id| !dictionary.contains_key(id)) {
        return Err(RepositoryError::CorruptState);
    }
    dictionary.retain(|id, _| referenced.contains(id));
    Ok(())
}

fn collect_refs(value: &Value, ids: &mut BTreeSet<String>) -> Result<(), RepositoryError> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(
                    key.as_str(),
                    "evidenceRef"
                        | "epistemicUntilEvidenceRef"
                        | "leftEvidenceRef"
                        | "rightEvidenceRef"
                ) {
                    ids.insert(
                        value
                            .as_str()
                            .ok_or(RepositoryError::CorruptState)?
                            .to_owned(),
                    );
                } else {
                    collect_refs(value, ids)?;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_refs(item, ids)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}
