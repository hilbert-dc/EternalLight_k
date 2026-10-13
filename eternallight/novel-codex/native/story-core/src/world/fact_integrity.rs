use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

// Shared by authority reads and background audits. Missing joins are false, never valid.
pub(crate) const FACT_LEDGER_SELECT: &str = "SELECT f.rowid, f.fact_id, f.payload_json, \
  COALESCE(a.project_id = f.project_id AND a.operation = 'accept' \
    AND a.result_revision = f.project_revision \
    AND accepted_snapshot.project_id = f.project_id \
    AND accepted_snapshot.branch_id = a.branch_id \
    AND f.reverted_by_commit_id IS a.undone_by_commit_id \
    AND (f.reverted_by_commit_id IS NULL OR ( \
      undone.project_id = f.project_id AND undone.operation = 'undo' \
      AND undone.target_commit_id = f.accepted_by_commit_id \
      AND undone.branch_id = a.branch_id \
      AND undone.result_revision > f.project_revision \
      AND undone_snapshot.project_id = f.project_id \
      AND undone_snapshot.branch_id = a.branch_id)), 0) \
  FROM story_facts_v2 f \
  LEFT JOIN story_acceptances_v2 a ON a.commit_id = f.accepted_by_commit_id \
  LEFT JOIN story_snapshots_v2 accepted_snapshot ON accepted_snapshot.snapshot_id = a.result_snapshot_id \
  LEFT JOIN story_acceptances_v2 undone ON undone.commit_id = f.reverted_by_commit_id \
  LEFT JOIN story_snapshots_v2 undone_snapshot ON undone_snapshot.snapshot_id = undone.result_snapshot_id";

pub(crate) struct FactLedgerRow {
    pub rowid: i64,
    pub fact_id: String,
    payload_json: String,
    links_valid: bool,
}

impl FactLedgerRow {
    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            rowid: row.get(0)?,
            fact_id: row.get(1)?,
            payload_json: row.get(2)?,
            links_valid: row.get(3)?,
        })
    }

    pub(crate) fn payload_valid(&self) -> bool {
        let Ok(payload) = serde_json::from_str::<Value>(&self.payload_json) else {
            return false;
        };
        payload["id"].as_str() == Some(self.fact_id.as_str())
            && (payload.get("modelingBatchId").is_some()
                == payload.get("modelingAssertionIndex").is_some())
            && (payload.get("modelingBatchId").is_none()
                || (payload["modelingBatchId"].as_str().is_some()
                    && payload["modelingAssertionIndex"].as_u64().is_some()))
    }

    pub(crate) fn links_valid(&self) -> bool {
        self.links_valid
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FactLedgerAudit {
    pub facts: usize,
    pub invalid_links: usize,
    pub invalid_payloads: usize,
    pub affected_facts: Vec<String>,
}

impl FactLedgerAudit {
    pub fn complete(&self) -> bool {
        self.invalid_links == 0 && self.invalid_payloads == 0
    }

    pub(crate) fn empty() -> Self {
        Self {
            facts: 0,
            invalid_links: 0,
            invalid_payloads: 0,
            affected_facts: Vec::new(),
        }
    }

    pub(crate) fn record(&mut self, row: &FactLedgerRow) {
        self.facts += 1;
        let links_valid = row.links_valid();
        let payload_valid = row.payload_valid();
        self.invalid_links += usize::from(!links_valid);
        self.invalid_payloads += usize::from(!payload_valid);
        if (!links_valid || !payload_valid) && self.affected_facts.len() < 20 {
            self.affected_facts.push(row.fact_id.clone());
        }
    }
}



fn audit_on(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<FactLedgerAudit, RepositoryError> {
    let sql = format!("{FACT_LEDGER_SELECT} WHERE f.project_id = ?1 ORDER BY f.rowid");
    let mut statement = connection.prepare(&sql).map_err(storage_error)?;
    let rows = statement
        .query_map(params![project_id.as_str()], FactLedgerRow::from_row)
        .map_err(storage_error)?;
    let mut audit = FactLedgerAudit::empty();
    for row in rows {
        audit.record(&row.map_err(storage_error)?);
    }
    Ok(audit)
}

pub(crate) fn validate_fact_ledger(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<(), RepositoryError> {
    // This decision and the subsequent fact read must share one SQLite snapshot.
    // Any authority change increments the generation before its transaction commits.
    let certificate: Option<(i64, String)> = connection
        .query_row(
            "SELECT j.source_count, j.audit_json FROM story_fact_audit_jobs_v1 j \
             WHERE j.project_id = ?1 AND j.status = 'completed' \
               AND j.source_generation = COALESCE((SELECT generation FROM story_fact_audit_state_v1 WHERE project_id = ?1), 0) \
             ORDER BY j.rowid DESC LIMIT 1",
            [project_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    if let Some((count, raw)) = certificate
        && let Ok(audit) = serde_json::from_str::<FactLedgerAudit>(&raw)
        && i64::try_from(audit.facts).ok() == Some(count)
    {
        return if audit.complete() {
            Ok(())
        } else {
            Err(RepositoryError::CorruptState)
        };
    }
    let sql = format!("{FACT_LEDGER_SELECT} WHERE f.project_id = ?1 ORDER BY f.rowid");
    let mut statement = connection.prepare(&sql).map_err(storage_error)?;
    let rows = statement
        .query_map(params![project_id.as_str()], FactLedgerRow::from_row)
        .map_err(storage_error)?;
    for row in rows {
        let row = row.map_err(storage_error)?;
        if !row.links_valid() || !row.payload_valid() {
            return Err(RepositoryError::CorruptState);
        }
    }
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// Public fact integrity capability; SQLite retains transaction ownership.
pub trait FactIntegrityRepository {
    fn audit_fact_ledger(&self, project_id: &ProjectId)
    -> Result<FactLedgerAudit, RepositoryError>;
}

impl FactIntegrityRepository for SqliteStoryRepository {
    fn audit_fact_ledger(
        &self,
        project_id: &ProjectId,
    ) -> Result<FactLedgerAudit, RepositoryError> {
        self.project_revision(project_id)?;
        self.with_read_snapshot(|| audit_on(&self.connection, project_id))?
    }
}

use crate::{ProjectRevisionRepository, ReadSnapshotRepository};
