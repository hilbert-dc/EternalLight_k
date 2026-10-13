use crate::{Result, digest, evidence, evidence_selection};
use serde_json::{Value, json};

/// Immutable owned evidence. JSON snapshots are exports, never input authority
/// for subsequent selection. Report loading must bind every qualifying record.
pub struct EvidenceSnapshot {
    records: Value,
}

impl EvidenceSnapshot {
    pub fn new(records: &Value) -> Result<Self> {
        let records = records
            .as_array()
            .filter(|v| v.len() <= 10_000)
            .ok_or("evidence records must be a bounded array")?;
        let normalized: Vec<_> = records
            .iter()
            .map(|v| evidence::dispatch("evidence.normalize", v))
            .collect::<Result<_>>()?;
        Ok(Self {
            records: evidence::dispatch("evidence.effective", &json!(normalized))?,
        })
    }

    pub fn report_refs(records: &Value) -> Result<Value> {
        let mut refs = std::collections::BTreeSet::new();
        for raw in records
            .as_array()
            .filter(|v| v.len() <= 10_000)
            .ok_or("evidence repository records must be a bounded array")?
        {
            let record = evidence::dispatch("evidence.normalize", raw)?;
            if record["status"] != "verified" && record["source"] != "conformance-suite" {
                continue;
            }
            let reference = record["reportRef"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    format!(
                        "evidence report binding is invalid: {}",
                        record["evidenceRef"]
                    )
                })?;
            if record["reportDigest"] != reference {
                return Err(format!(
                    "evidence report binding is invalid: {}",
                    record["evidenceRef"]
                ));
            }
            refs.insert(reference.to_owned());
        }
        Ok(json!(refs))
    }

    pub fn from_reports(input: &Value) -> Result<Self> {
        Self::report_refs(&input["records"])?;
        for raw in input["records"]
            .as_array()
            .ok_or("invalid evidence records")?
        {
            let mut record = evidence::dispatch("evidence.normalize", raw)?;
            if record["status"] != "verified" && record["source"] != "conformance-suite" {
                continue;
            }
            let report = &input["reports"][record["reportRef"]
                .as_str()
                .ok_or("invalid evidence report reference")?];
            if report["reportId"] != record["reportDigest"] {
                return Err(format!(
                    "evidence report digest mismatch: {}",
                    record["evidenceRef"]
                ));
            }
            let mut report_body = report.clone();
            report_body
                .as_object_mut()
                .ok_or("invalid evidence report")?
                .remove("reportId");
            if digest::digest(&report_body) != report["reportId"] {
                return Err("evidence report digest mismatch".into());
            }
            let matches: Vec<_> = report["records"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| r["evidenceRef"] == record["evidenceRef"])
                .collect();
            if matches.len() != 1 {
                return Err(
                    "evidence is not bound to its report or fields have been tampered".into(),
                );
            }
            let mut candidate = matches[0].clone();
            candidate["reportRef"] = record["reportRef"].clone();
            candidate["reportDigest"] = record["reportDigest"].clone();
            candidate = evidence::dispatch("evidence.normalize", &candidate)?;
            for value in [&mut record, &mut candidate] {
                let object = value.as_object_mut().ok_or("invalid normalized evidence")?;
                object.remove("reportRef");
                object.remove("reportDigest");
            }
            if record != candidate {
                return Err(
                    "evidence is not bound to its report or fields have been tampered".into(),
                );
            }
        }
        Self::new(&input["records"])
    }

    pub fn apply(&self, method: &str, input: &Value) -> Result<Value> {
        match method {
            "session" | "protocols" => {
                let mut request = input.clone();
                request["records"] = self.records.clone();
                let kind = if method == "protocols" {
                    crate::session_identity::Compilation::Protocols
                } else {
                    crate::session_identity::Compilation::Plan
                };
                crate::session_identity::compile(&request, kind)
            }
            "snapshot" => {
                let created_at = input["createdAt"]
                    .as_str()
                    .ok_or("evidence snapshot time is invalid")?;
                chrono::DateTime::parse_from_rfc3339(created_at)
                    .map_err(|_| "evidence snapshot time is invalid")?;
                Ok(
                    json!({"createdAt":created_at,"digest":digest::digest(&self.records),"records":self.records}),
                )
            }
            "select" | "credentialFailure" => {
                let mut request = input.clone();
                request["records"] = self.records.clone();
                if method == "select" {
                    evidence_selection::select(&request)
                } else {
                    evidence_selection::credential_failure(&request)
                }
            }
            _ => Err("unknown evidence snapshot operation".into()),
        }
    }
}
