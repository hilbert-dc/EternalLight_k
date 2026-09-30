use std::collections::BTreeMap;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::RepositoryError;
use crate::query_plan::{QueryOperation, QueryStep};

pub(crate) fn step_report(step: &QueryStep, mut value: Value) -> Result<Value, RepositoryError> {
    let (rows_key, mut coverage) = match &step.operation {
        QueryOperation::ResolveEntity { .. } => (
            "entries",
            json!({"complete":value["truncated"] == false,
            "catalogTruncated":value["truncated"],"requiresDisambiguation":value["ambiguousNames"].as_array().is_some_and(|names| !names.is_empty()),
            "identityRevisionId":value["identityRevisionId"]}),
        ),
        QueryOperation::State { .. } => ("entries", value["coverage"].clone()),
        QueryOperation::Text { .. }
        | QueryOperation::Passages { .. }
        | QueryOperation::Relations { .. } => ("results", value["coverage"].clone()),
    };
    if coverage["requiresDisambiguation"] == true {
        coverage["complete"] = json!(false);
    }
    // IDs are optional diagnostics, counts and truncation remain authoritative.
    if coverage["pendingSources"].is_array() {
        coverage["pendingSourceIdsWithheld"] = json!(true);
        coverage
            .as_object_mut()
            .ok_or(RepositoryError::CorruptState)?
            .remove("pendingSources");
    }
    let rows = value
        .get_mut(rows_key)
        .and_then(Value::as_array_mut)
        .ok_or(RepositoryError::CorruptState)?;
    let mut results = std::mem::take(rows);
    for (index, row) in results.iter_mut().enumerate() {
        row["resultId"] = json!(format!(
            "{:x}",
            Sha256::digest(format!("{}:{index}:{}", step.id, row).as_bytes())
        ));
        if matches!(step.operation, QueryOperation::Text { .. }) {
            row["sourceObjectId"] = row["id"].clone();
        }
    }
    let mut findings = value["findings"].as_array().cloned().unwrap_or_default();
    for finding in &mut findings {
        for side in ["left", "right"] {
            let index_key = format!("{side}EntryIndex");
            let index = finding[&index_key]
                .as_u64()
                .ok_or(RepositoryError::CorruptState)? as usize;
            finding[format!("{side}ResultId")] =
                results.get(index).ok_or(RepositoryError::CorruptState)?["resultId"].clone();
            finding
                .as_object_mut()
                .ok_or(RepositoryError::CorruptState)?
                .remove(&index_key);
        }
    }
    Ok(
        json!({"stepId":step.id,"results":results,"findings":findings,"coverage":coverage,
        "originalReturnedItems":results.len(),"returnedItems":results.len(),"responseTruncated":false,
        "nextCursor":value["nextCursor"]}),
    )
}

pub(crate) fn fit_report(report: &mut Value, budget: usize) -> Result<(), RepositoryError> {
    crate::query_ranking::rank(report)?;
    let mut evidence = BTreeMap::new();
    for step in report["steps"]
        .as_array_mut()
        .ok_or(RepositoryError::CorruptState)?
    {
        crate::query_evidence::normalize(&mut step["results"], &mut evidence)?;
        let rows = step["results"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
            .clone();
        for finding in step["findings"]
            .as_array_mut()
            .ok_or(RepositoryError::CorruptState)?
        {
            for side in ["left", "right"] {
                let row = rows
                    .iter()
                    .find(|row| row["resultId"] == finding[format!("{side}ResultId")])
                    .ok_or(RepositoryError::CorruptState)?;
                let reference = row["assertion"]["evidenceRef"]
                    .as_str()
                    .ok_or(RepositoryError::CorruptState)?;
                finding[format!("{side}EvidenceRef")] = json!(reference);
                finding
                    .as_object_mut()
                    .ok_or(RepositoryError::CorruptState)?
                    .remove(&format!("{side}Evidence"));
            }
        }
    }
    report["evidence"] = json!(evidence);
    crate::query_evidence::retain_referenced(report)?;
    bound_report(report, budget)
}

pub(crate) fn bound_report(report: &mut Value, budget: usize) -> Result<(), RepositoryError> {
    // Reserve the hash's final serialized size before choosing which whole rows fit.
    report["contentHash"] = json!("0".repeat(64));
    while report.to_string().len() > budget
        || report["evidence"]
            .as_object()
            .ok_or(RepositoryError::CorruptState)?
            .len()
            > 40
    {
        let steps = report["steps"]
            .as_array_mut()
            .ok_or(RepositoryError::CorruptState)?;
        let victim = steps
            .iter()
            .enumerate()
            .flat_map(|(step_index, step)| {
                let rows = step["results"].as_array().into_iter().flatten();
                let count = step["results"].as_array().map_or(0, Vec::len);
                rows.enumerate().map(move |(row_index, row)| {
                    (
                        (
                            usize::from(count == 1),
                            row["relevanceScore"].as_u64().unwrap_or_default(),
                            std::cmp::Reverse(row.to_string().len()),
                            step_index,
                            row_index,
                        ),
                        (step_index, row_index),
                    )
                })
            })
            .min_by_key(|(priority, _)| *priority)
            .map(|(_, position)| position);
        let Some((step_index, row_index)) = victim else {
            return Err(RepositoryError::InvalidContent(
                "query report metadata exceeds byte budget".into(),
            ));
        };
        let step = &mut steps[step_index];
        step["results"]
            .as_array_mut()
            .ok_or(RepositoryError::CorruptState)?
            .remove(row_index);
        step["responseTruncated"] = json!(true);
        step["coverage"]["complete"] = json!(false);
        if step["coverage"].get("pageComplete").is_some() {
            step["coverage"]["pageComplete"] = json!(false);
            step["coverage"]["endOfScopeFromCursor"] = json!(false);
        }
        // The original cursor would skip rows omitted by this package.
        if !step["nextCursor"].is_null() {
            step["nextCursor"] = Value::Null;
            step["coverage"]["cursorWithheldAfterBudget"] = json!(true);
        }
        step["coverage"]["requeryRequired"] = json!(true);
        step["returnedItems"] = json!(step["results"].as_array().map_or(0, Vec::len));
        step["findings"] = json!([]);
        crate::query_evidence::retain_referenced(report)?;
        report["coverage"]["responseTruncated"] = json!(true);
        report["coverage"]["complete"] = json!(false);
    }
    report["contentHash"] = json!("");
    let digest = format!("{:x}", Sha256::digest(report.to_string().as_bytes()));
    report["contentHash"] = json!(digest);
    Ok(())
}

#[cfg(test)]
#[path = "query_report_tests.rs"]
mod tests;
