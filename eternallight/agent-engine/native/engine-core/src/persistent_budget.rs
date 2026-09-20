use crate::{Result, budget, storage};
use rusqlite::{Connection, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS engine_budgets (budget_id TEXT PRIMARY KEY, run_id TEXT NOT NULL UNIQUE, status TEXT NOT NULL, state_json TEXT NOT NULL, updated_at INTEGER NOT NULL) STRICT;";

fn open(path: &Path) -> Result<Connection> {
    let connection = Connection::open(path).map_err(|e| e.to_string())?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch(SCHEMA)
        .map_err(|e| e.to_string())?;
    Ok(connection)
}

fn owner_path(path: &Path, run: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    let digest = format!("{:x}", Sha256::digest(run));
    name.push(format!(".{digest}.owner"));
    name.into()
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

/// SQLite commits a candidate state before publishing it to the active caller.
/// A held OS lock distinguishes active budgets from crashed owners during recovery.
pub struct PersistentBudget {
    database: Option<Connection>,
    owner: Option<File>,
    state: Value,
    poisoned: bool,
}

impl PersistentBudget {
    pub fn new(path: &Path, run_id: &str, input: Value) -> Result<Self> {
        if path.as_os_str().is_empty() {
            return Err("persistent budget database path is required".into());
        }
        if run_id.is_empty() {
            return Err("persistent budget run id is required".into());
        }
        let owner = storage::try_own(&owner_path(path, run_id))?
            .ok_or("persistent budget run already has an owner")?;
        let mut database = open(path)?;
        let state = budget::dispatch("budget.create", input)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        transaction.execute("INSERT INTO engine_budgets (budget_id, run_id, status, state_json, updated_at) VALUES (?, ?, 'active', ?, ?)",params![state["id"].as_str(),run_id,state.to_string(),now()]).map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())?;
        Ok(Self {
            database: Some(database),
            owner: Some(owner),
            state,
            poisoned: false,
        })
    }

    pub fn apply(&mut self, method: &str, mut input: Value) -> Result<Value> {
        if self.poisoned {
            return Err("persistent budget requires recovery after storage failure".into());
        }
        if self.database.is_none() {
            return Err("persistent budget is closed".into());
        }
        if !input.is_object() {
            return Err("budget parameters must be an object".into());
        }
        input["state"] = self.state.clone();
        let candidate = budget::dispatch(method, input)?;
        if !matches!(method, "budget.snapshot" | "budget.remaining") {
            if let Err(error) = self.persist(&candidate["state"], "active") {
                self.poisoned = true;
                return Err(error);
            }
            self.state = candidate["state"].clone();
        }
        Ok(candidate["result"].clone())
    }

    fn persist(&mut self, state: &Value, status: &str) -> Result<()> {
        let database = self
            .database
            .as_mut()
            .ok_or("persistent budget is closed")?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let changed = transaction.execute("UPDATE engine_budgets SET status = ?, state_json = ?, updated_at = ? WHERE budget_id = ? AND status = 'active'",params![status,state.to_string(),now(),state["id"].as_str()]).map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("persistent budget lost its active record".into());
        }
        transaction.commit().map_err(|e| e.to_string())
    }

    pub fn complete(&mut self, status: &str) -> Result<()> {
        if !matches!(
            status,
            "terminal" | "completed" | "failed" | "cancelled" | "outcome-unknown"
        ) {
            return Err("invalid persistent budget terminal status".into());
        }
        if self.poisoned {
            return Err("persistent budget requires recovery after storage failure".into());
        }
        let candidate = budget::dispatch("budget.unknown", json!({"state":self.state}))?;
        if let Err(error) = self.persist(&candidate["state"], status) {
            self.poisoned = true;
            return Err(error);
        }
        self.state = candidate["state"].clone();
        self.close();
        Ok(())
    }

    pub fn inspect(&self) -> Value {
        self.state.clone()
    }
    pub fn close(&mut self) {
        self.database.take();
        self.owner.take();
    }

    pub fn recover(path: &Path) -> Result<Value> {
        let mut database = open(path)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let rows = {
            let mut statement = transaction.prepare("SELECT budget_id, run_id, state_json FROM engine_budgets WHERE status = 'active' ORDER BY rowid").map_err(|e| e.to_string())?;
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| e.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| e.to_string())?
        };
        let mut recovered = Vec::new();
        let mut owners = Vec::new();
        for (id, run, state) in rows {
            let Some(owner) = storage::try_own(&owner_path(path, &run))? else {
                continue;
            };
            owners.push(owner);
            let state: Value =
                serde_json::from_str(&state).map_err(|_| "invalid persisted budget")?;
            let updated = budget::dispatch("budget.unknown", json!({"state":state}))?;
            let state = &updated["state"];
            transaction.execute("UPDATE engine_budgets SET status = 'recovered_unknown', state_json = ?, updated_at = ? WHERE budget_id = ?",params![state.to_string(),now(),id]).map_err(|e| e.to_string())?;
            recovered.push(json!({"budgetId":id,"unknownTokens":state["unknownTokens"],"overrun":state["overrun"]}));
        }
        transaction.commit().map_err(|e| e.to_string())?;
        Ok(recovered.into())
    }
}
