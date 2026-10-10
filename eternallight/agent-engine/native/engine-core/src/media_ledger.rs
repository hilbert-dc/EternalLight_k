use crate::{Result, storage};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    path::{Path, PathBuf},
    time::Duration,
};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS engine_media_grants (
 grant_id TEXT PRIMARY KEY, definition TEXT NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS engine_media_operations (
 operation_id TEXT PRIMARY KEY, grant_id TEXT NOT NULL REFERENCES engine_media_grants(grant_id),
 request_digest TEXT NOT NULL, idempotency_key TEXT NOT NULL, record TEXT NOT NULL,
 UNIQUE(grant_id, idempotency_key)
) STRICT;
CREATE INDEX IF NOT EXISTS engine_media_by_grant ON engine_media_operations(grant_id);
CREATE TABLE IF NOT EXISTS engine_media_resource_policy (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1), global_limit INTEGER NOT NULL, pool_limit INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS engine_media_admissions (
 operation_id TEXT PRIMARY KEY REFERENCES engine_media_operations(operation_id), pool_key TEXT NOT NULL,
 owner_id TEXT NOT NULL, fence INTEGER NOT NULL, lease_expires_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS engine_media_admissions_pool ON engine_media_admissions(pool_key);
"#;

fn identity<'a>(input: &'a Value, key: &str) -> Result<&'a str> {
    input[key]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 200 && !v.chars().any(char::is_control))
        .ok_or_else(|| format!("invalid media {key}"))
}
fn integer(input: &Value, key: &str) -> Result<u64> {
    input[key]
        .as_u64()
        .filter(|n| *n <= 9_007_199_254_740_991)
        .ok_or_else(|| format!("invalid media {key}"))
}
fn units(value: &Value) -> Result<()> {
    let map = value.as_object().ok_or("media units must be an object")?;
    if map.len() > 6 {
        return Err("too many media unit dimensions".into());
    }
    for (key, amount) in map {
        if !matches!(
            key.as_str(),
            "images"
                | "videoMilliseconds"
                | "audioMilliseconds"
                | "characters"
                | "renderFrames"
                | "bytes"
        ) || amount.as_u64().is_none_or(|n| n > 9_007_199_254_740_991)
        {
            return Err("invalid media unit".into());
        }
    }
    Ok(())
}
fn load(db: &Connection, id: &str) -> Result<Value> {
    let value: Option<String> = db
        .query_row(
            "SELECT record FROM engine_media_operations WHERE operation_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&value.ok_or("media operation not found")?)
        .map_err(|_| "corrupt media operation".into())
}
fn save(db: &Connection, record: &mut Value, now: u64) -> Result<()> {
    record["version"] = (integer(record, "version")? + 1).into();
    record["updatedAt"] = now.into();
    db.execute(
        "UPDATE engine_media_operations SET record=?1 WHERE operation_id=?2",
        params![record.to_string(), record["id"].as_str()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn terminal(state: &str) -> bool {
    matches!(state, "ready" | "failed" | "cancelled")
}
fn resource_pool(record: &Value) -> Result<String> {
    let connection = &record["connection"];
    let realm = identity(connection, "credentialRealmId")?;
    let mut endpoint = url::Url::parse(identity(connection, "baseUrl")?)
        .map_err(|_| "invalid media resource endpoint")?;
    if !matches!(endpoint.scheme(), "http" | "https")
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err("invalid media resource endpoint".into());
    }
    let path = endpoint.path().trim_end_matches('/').to_owned();
    endpoint.set_path(&path);
    let family = identity(record, "kind")?
        .split('.')
        .next()
        .ok_or("invalid media resource kind")?;
    Ok(crate::digest::digest(
        &json!({"credentialRealmId":realm,"endpoint":endpoint.as_str(),"family":family}),
    ))
}
fn dispatch_permitted(db: &Connection, record: &Value, now: u64) -> Result<()> {
    if record["cancelRequested"] == true {
        return Err("media operation cancelled before dispatch".into());
    }
    let grant: String = db
        .query_row(
            "SELECT definition FROM engine_media_grants WHERE grant_id=?1",
            [record["grantId"].as_str()],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let grant: Value = serde_json::from_str(&grant).map_err(|_| "corrupt media grant")?;
    if integer(&grant, "expiresAt")? <= now {
        return Err("media grant expired".into());
    }
    Ok(())
}
fn release_admission(db: &Connection, record: &Value) -> Result<bool> {
    // A local lease ending does not prove a remote job stopped.
    let state = record["state"].as_str().ok_or("invalid media state")?;
    if state == "prepared" || terminal(state) {
        db.execute(
            "DELETE FROM engine_media_admissions WHERE operation_id=?1",
            [record["id"].as_str()],
        )
        .map_err(|e| e.to_string())?;
        return Ok(true);
    }
    Ok(false)
}
fn admit_resource(db: &Connection, record: &Value, input: &Value, now: u64) -> Result<Value> {
    let global = integer(input, "globalLimit")?;
    let pool = integer(input, "poolLimit")?;
    if global == 0 || global > 256 || pool == 0 || pool > global {
        return Err("invalid media resource limits".into());
    }
    let policy: Option<(u64, u64)> = db
        .query_row(
            "SELECT global_limit,pool_limit FROM engine_media_resource_policy WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(policy) = policy {
        if policy != (global, pool) {
            return Err("media resource policy mismatch across hosts".into());
        }
    } else {
        db.execute(
            "INSERT INTO engine_media_resource_policy VALUES (1,?1,?2)",
            params![global, pool],
        )
        .map_err(|e| e.to_string())?;
    }
    let state = record["state"].as_str().ok_or("invalid media state")?;
    if terminal(state) {
        return Err("terminal media does not acquire resources".into());
    }
    if state == "prepared" {
        dispatch_permitted(db, record, now)?;
    }
    let pool_key = resource_pool(record)?;
    let rows = {
        let mut statement = db
            .prepare("SELECT operation_id,pool_key,lease_expires_at FROM engine_media_admissions")
            .map_err(|e| e.to_string())?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?
    };
    let mut count = 0;
    let mut in_pool = 0;
    let mut existing = false;
    for (id, key, expiry) in rows {
        let operation = load(db, &id)?;
        let state = operation["state"].as_str().ok_or("invalid media state")?;
        if terminal(state) || state == "prepared" && expiry <= now {
            db.execute(
                "DELETE FROM engine_media_admissions WHERE operation_id=?1",
                [&id],
            )
            .map_err(|e| e.to_string())?;
            continue;
        }
        if operation["id"] == record["id"] {
            if key != pool_key {
                return Err("media resource pool changed".into());
            }
            existing = true;
        }
        count += 1;
        if key == pool_key {
            in_pool += 1;
        }
    }
    if !existing && (count >= global || in_pool >= pool) {
        return Ok(json!({"admitted":false,"retryAt":now+5000,"reason":"capacity"}));
    }
    db.execute("INSERT INTO engine_media_admissions VALUES (?1,?2,?3,?4,?5) ON CONFLICT(operation_id) DO UPDATE SET owner_id=excluded.owner_id,fence=excluded.fence,lease_expires_at=excluded.lease_expires_at",params![record["id"].as_str(),pool_key,record["ownerId"].as_str(),integer(record,"fence")?,integer(record,"leaseExpiresAt")?]).map_err(|e|e.to_string())?;
    Ok(
        json!({"admitted":true,"poolKey":pool_key,"ownerId":record["ownerId"],"fence":record["fence"],"leaseExpiresAt":record["leaseExpiresAt"]}),
    )
}
fn transition_allowed(from: &str, to: &str) -> bool {
    from == to
        || match from {
            "prepared" => matches!(to, "submitting" | "failed" | "cancelled"),
            "submitting" => matches!(
                to,
                "submitted" | "running" | "collecting" | "failed" | "outcome_unknown" | "cancelled"
            ),
            "submitted" | "running" => matches!(
                to,
                "running" | "collecting" | "failed" | "outcome_unknown" | "cancelled"
            ),
            "collecting" => matches!(to, "ready" | "failed" | "outcome_unknown"),
            "outcome_unknown" => matches!(
                to,
                "submitted" | "running" | "collecting" | "failed" | "cancelled" | "ready"
            ),
            _ => false,
        }
}

/// External operation state and media budget dimensions share the engine budget
/// database. Domain documents and assets remain exclusively in the product Runtime.
pub struct MediaLedger {
    db: Connection,
    path: PathBuf,
    owners: BTreeMap<String, File>,
}

impl MediaLedger {
    pub fn new(path: &Path) -> Result<Self> {
        if path.as_os_str().is_empty() {
            return Err("media ledger path is required".into());
        }
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        db.busy_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
        )
        .map_err(|e| e.to_string())?;
        db.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        Ok(Self {
            db,
            path: path.into(),
            owners: BTreeMap::new(),
        })
    }
    pub fn apply(&mut self, method: &str, input: &Value) -> Result<Value> {
        let now = integer(input, "now")?;
        if method == "prepare" {
            return self.prepare(input, now);
        }
        if method == "list" {
            let mut stmt = self
                .db
                .prepare("SELECT record FROM engine_media_operations ORDER BY operation_id")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            let mut result = Vec::new();
            for row in rows {
                let record: Value = serde_json::from_str(&row.map_err(|e| e.to_string())?)
                    .map_err(|_| "corrupt media operation")?;
                if input["projectId"].is_string() && record["projectId"] != input["projectId"] {
                    continue;
                }
                if input["unfinishedOnly"] == true
                    && terminal(record["state"].as_str().unwrap_or(""))
                {
                    continue;
                }
                result.push(record);
            }
            return Ok(result.into());
        }
        let id = identity(input, "id")?.to_owned();
        if method == "find" {
            let raw: Option<String> = self
                .db
                .query_row(
                    "SELECT record FROM engine_media_operations WHERE operation_id=?1",
                    [&id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            return match raw {
                Some(value) => {
                    serde_json::from_str(&value).map_err(|_| "corrupt media operation".into())
                }
                None => Ok(Value::Null),
            };
        }
        if method == "get" {
            return load(&self.db, &id);
        }
        if method == "release" {
            if self.owners.contains_key(&id) {
                let record = load(&self.db, &id)?;
                release_admission(&self.db, &record)?;
            }
            self.owners.remove(&id);
            return Ok(Value::Null);
        }
        if method == "claim" {
            if self.owners.contains_key(&id) {
                return Err("media operation already owned by this handle".into());
            }
            let mut lock_path = self.path.as_os_str().to_owned();
            lock_path.push(format!(".media-{:x}.owner", Sha256::digest(id.as_bytes())));
            let lock = storage::try_own(Path::new(&lock_path))?
                .ok_or("media operation already has an owner")?;
            let tx = self
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let mut record = load(&tx, &id)?;
            // Terminal output is immutable, but ownership is still needed to
            // reconcile a late authoritative billing receipt.
            record["ownerId"] = identity(input, "ownerId")?.into();
            record["fence"] = (integer(&record, "fence")? + 1).into();
            let duration = integer(input, "leaseMs")?;
            if duration == 0 || duration > 3_600_000 {
                return Err("invalid media lease duration".into());
            }
            record["leaseExpiresAt"] = now
                .checked_add(duration)
                .ok_or("media time overflow")?
                .into();
            save(&tx, &mut record, now)?;
            tx.commit().map_err(|e| e.to_string())?;
            self.owners.insert(id, lock);
            return Ok(record);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let mut record = load(&tx, &id)?;
        if method == "cancel" {
            record["cancelRequested"] = true.into();
            save(&tx, &mut record, now)?;
        } else {
            if !self.owners.contains_key(&id)
                || record["ownerId"] != input["ownerId"]
                || record["fence"] != input["fence"]
                || method != "releaseAdmission" && integer(&record, "leaseExpiresAt")? <= now
            {
                return Err("media execution ownership is stale".into());
            }
            if !matches!(method, "admit" | "releaseAdmission")
                && input["expectedVersion"] != record["version"]
            {
                return Err("media operation version conflict".into());
            }
            match method {
                "admit" => {
                    let result = admit_resource(&tx, &record, input, now)?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(result);
                }
                "releaseAdmission" => {
                    let released = release_admission(&tx, &record)?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(json!({"released":released,"heldForRemote":!released}));
                }
                "renew" => {
                    let duration = integer(input, "leaseMs")?;
                    if duration == 0 || duration > 3_600_000 {
                        return Err("invalid media lease duration".into());
                    }
                    record["leaseExpiresAt"] = now
                        .checked_add(duration)
                        .ok_or("media time overflow")?
                        .into();
                    tx.execute("UPDATE engine_media_admissions SET lease_expires_at=?1 WHERE operation_id=?2 AND owner_id=?3 AND fence=?4",params![integer(&record,"leaseExpiresAt")?,id,record["ownerId"].as_str(),integer(&record,"fence")?]).map_err(|e|e.to_string())?;
                }
                "defer" => {
                    if terminal(record["state"].as_str().ok_or("invalid media state")?) {
                        return Err("media result is terminal".into());
                    }
                    record["nextPollAt"] = integer(input, "nextPollAt")?.into();
                    if !input["errorCode"].is_null() {
                        record["errorCode"] = identity(input, "errorCode")?.into();
                    }
                }
                "transition" => {
                    let from = record["state"].as_str().ok_or("corrupt media state")?;
                    let to = identity(input, "state")?;
                    if terminal(from) {
                        return Err("media result is terminal".into());
                    }
                    if !transition_allowed(from, to) {
                        return Err("invalid media operation transition".into());
                    }
                    if to == "submitting" {
                        if from != "prepared" {
                            return Err("media submission cannot be repeated".into());
                        }
                        dispatch_permitted(&tx, &record, now)?;
                        let admitted:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM engine_media_admissions WHERE operation_id=?1 AND owner_id=?2 AND fence=?3 AND lease_expires_at>?4)",params![id,record["ownerId"].as_str(),integer(&record,"fence")?,now],|row|row.get(0)).map_err(|e|e.to_string())?;
                        if !admitted {
                            return Err("media dispatch requires current resource admission".into());
                        }
                        record["dispatchAttempted"] = true.into();
                    }
                    let patch = input["patch"]
                        .as_object()
                        .ok_or("invalid media operation patch")?;
                    for (key, value) in patch {
                        if !matches!(
                            key.as_str(),
                            "providerJobId"
                                | "receipt"
                                | "outputAssetRevisionIds"
                                | "resultReceipt"
                                | "errorCode"
                                | "nextPollAt"
                        ) {
                            return Err("media operation patch changes immutable identity".into());
                        }
                        if key == "providerJobId" && !record[key].is_null() && record[key] != *value
                        {
                            return Err("media provider job identity changed".into());
                        }
                        if key == "providerJobId" {
                            identity(&json!({"id":value}), "id")?;
                        }
                        if value.to_string().len() > 128 * 1024 {
                            return Err("media receipt exceeds limit".into());
                        }
                        record[key] = value.clone();
                    }
                    let has_assets = record["outputAssetRevisionIds"]
                        .as_array()
                        .is_some_and(|a| {
                            !a.is_empty()
                                && a.iter().all(|id| {
                                    id.as_str().is_some_and(|v| !v.is_empty() && v.len() <= 200)
                                })
                        });
                    let has_transcript = record["kind"] == "audio.transcribe"
                        && record["resultReceipt"]["outputs"]["artifactRevisionId"]
                            .as_str()
                            .is_some_and(|id| !id.is_empty() && id.len() <= 200);
                    if to == "ready"
                        && (!record["resultReceipt"].is_object() || !(has_assets || has_transcript))
                    {
                        return Err("media ready requires durable asset receipt".into());
                    }
                    record["state"] = to.into();
                    if terminal(to) {
                        release_admission(&tx, &record)?;
                    }
                }
                "settle" => {
                    let receipt = identity(input, "receiptId")?;
                    let outcome = identity(input, "outcome")?;
                    if !matches!(outcome, "not_sent" | "known" | "unknown") {
                        return Err("invalid media settlement outcome".into());
                    }
                    if outcome == "not_sent" && record["dispatchAttempted"] == true {
                        return Err("dispatched media cannot be refunded as not sent".into());
                    }
                    let usage = &input["usage"];
                    units(&usage["units"])?;
                    if !usage["costMinor"].is_null() {
                        integer(usage, "costMinor")?;
                    }
                    let settlement = json!({"receiptId":receipt,"outcome":outcome,"usage":usage});
                    if !record["settlement"].is_null() {
                        if record["settlement"] == settlement {
                            return Ok(record);
                        }
                        if record["settlement"]["outcome"] != "unknown" {
                            return Err("conflicting media settlement".into());
                        }
                    }
                    record["settlement"] = settlement;
                }
                _ => return Err("unknown media ledger operation".into()),
            }
            save(&tx, &mut record, now)?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(record)
    }

    fn prepare(&mut self, input: &Value, now: u64) -> Result<Value> {
        let grant = &input["grant"];
        let operation = &input["operation"];
        resource_pool(operation)?;
        let grant_id = identity(grant, "id")?;
        let id = identity(operation, "id")?;
        for key in ["projectId", "scopeId", "branchId", "workTaskId"] {
            identity(grant, key)?;
            if grant[key] != operation[key] {
                return Err("media operation grant scope mismatch".into());
            }
        }
        for key in ["attemptId", "providerId", "kind", "idempotencyKey"] {
            identity(operation, key)?;
        }
        let digest = identity(operation, "requestDigest")?;
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("invalid media request digest".into());
        }
        if !operation["request"].is_object() || operation["request"].to_string().len() > 512 * 1024
        {
            return Err("invalid media request".into());
        }
        if crate::digest::digest(&operation["request"]) != digest {
            return Err("media request digest mismatch".into());
        }
        if !matches!(
            operation["kind"].as_str(),
            Some(
                "image.generate"
                    | "image.edit"
                    | "audio.speech"
                    | "audio.transcribe"
                    | "video.generate"
                    | "local.render"
            )
        ) {
            return Err("unknown media operation kind".into());
        }
        let max_requests = integer(grant, "maxRequests")?;
        if max_requests == 0 || max_requests > 10000 {
            return Err("media grant is unavailable".into());
        }
        units(&grant["unitLimits"])?;
        units(&operation["reservation"]["units"])?;
        let reserved_units = &operation["reservation"]["units"];
        let minimum = match operation["kind"].as_str().unwrap_or("") {
            "image.generate" | "image.edit" => Some(("images", 1)),
            "video.generate" => Some((
                "videoMilliseconds",
                operation["request"]["duration"]
                    .as_u64()
                    .and_then(|n| n.checked_mul(1000))
                    .filter(|n| *n > 0)
                    .ok_or("invalid media video duration")?,
            )),
            "audio.speech" => Some((
                "characters",
                operation["request"]["text"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("invalid media speech text")?
                    .encode_utf16()
                    .count() as u64,
            )),
            "audio.transcribe" => Some(("audioMilliseconds", 1)),
            "local.render" => Some(("renderFrames", 1)),
            _ => None,
        };
        if minimum.is_some_and(|(key, min)| reserved_units[key].as_u64().is_none_or(|n| n < min)) {
            return Err("media reservation undercounts requested output".into());
        }
        if !operation["reservation"]["costMinor"].is_null() {
            integer(&operation["reservation"], "costMinor")?;
        }
        if !grant["maxCostMinor"].is_null() {
            integer(grant, "maxCostMinor")?;
            integer(&operation["reservation"], "costMinor")?;
            identity(grant, "currency")?;
            if operation["reservation"]["currency"] != grant["currency"] {
                return Err("media currency mismatch".into());
            }
        }
        if operation["reservation"]["currency"] != grant["currency"] {
            return Err("media currency mismatch".into());
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT record FROM engine_media_operations WHERE operation_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(existing) = existing {
            let record: Value =
                serde_json::from_str(&existing).map_err(|_| "corrupt media operation")?;
            if record["definition"] != *operation || record["grantId"] != grant_id {
                return Err("media operation idempotency conflict".into());
            }
            let original: String = tx
                .query_row(
                    "SELECT definition FROM engine_media_grants WHERE grant_id=?1",
                    [grant_id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if serde_json::from_str::<Value>(&original).map_err(|_| "corrupt media grant")?
                != *grant
            {
                return Err("media grant cannot be replaced or replenished".into());
            }
            return Ok(record);
        }
        if integer(grant, "expiresAt")? <= now {
            return Err("media grant is unavailable".into());
        }
        let existing: Option<String> = tx
            .query_row(
                "SELECT definition FROM engine_media_grants WHERE grant_id=?1",
                [grant_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(existing) = existing {
            let value: Value =
                serde_json::from_str(&existing).map_err(|_| "corrupt media grant")?;
            if value != *grant {
                return Err("media grant cannot be replaced or replenished".into());
            }
        } else {
            tx.execute(
                "INSERT INTO engine_media_grants VALUES (?1,?2)",
                params![grant_id, grant.to_string()],
            )
            .map_err(|e| e.to_string())?;
        }
        let mut count = 1_u64;
        let mut consumed = operation["reservation"]["units"]
            .as_object()
            .cloned()
            .unwrap();
        let mut cost = operation["reservation"]["costMinor"].as_u64().unwrap_or(0);
        {
            let mut stmt = tx
                .prepare("SELECT record FROM engine_media_operations WHERE grant_id=?1")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([grant_id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            for row in rows {
                let record: Value = serde_json::from_str(&row.map_err(|e| e.to_string())?)
                    .map_err(|_| "corrupt media operation")?;
                if record["settlement"]["outcome"] == "not_sent" {
                    continue;
                }
                count += 1;
                let reservation = &record["reservation"];
                let actual = &record["settlement"]["usage"];
                let known = record["settlement"]["outcome"] == "known";
                // A missing charge is unknown, never zero. Reserve the larger
                // observed amount until authoritative final usage is available.
                let reserved_cost = reservation["costMinor"].as_u64().unwrap_or(0);
                let actual_cost = actual["costMinor"].as_u64().unwrap_or(reserved_cost);
                cost = cost
                    .checked_add(if known {
                        actual_cost
                    } else {
                        actual_cost.max(reserved_cost)
                    })
                    .ok_or("media cost overflow")?;
                let mut dimensions = reservation["units"]
                    .as_object()
                    .ok_or("corrupt media reservation")?
                    .clone();
                if let Some(actual_units) = actual["units"].as_object() {
                    for (key, value) in actual_units {
                        dimensions.entry(key.clone()).or_insert(value.clone());
                    }
                }
                for (key, value) in &dimensions {
                    let reserved = value.as_u64().unwrap_or(0);
                    let observed = actual["units"][key].as_u64().unwrap_or(reserved);
                    let used = if known {
                        observed
                    } else {
                        observed.max(reserved)
                    };
                    let sum = consumed
                        .get(key)
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                        .checked_add(used)
                        .ok_or("media units overflow")?;
                    consumed.insert(key.clone(), sum.into());
                }
            }
        }
        if count > max_requests
            || grant["maxCostMinor"]
                .as_u64()
                .is_some_and(|limit| cost > limit)
        {
            return Err("media budget exhausted".into());
        }
        for (key, value) in &consumed {
            if grant["unitLimits"][key]
                .as_u64()
                .is_none_or(|limit| value.as_u64().unwrap_or(u64::MAX) > limit)
            {
                return Err(format!("media {key} budget exhausted"));
            }
        }
        let mut record = operation.clone();
        record["definition"] = operation.clone();
        record["grantId"] = grant_id.into();
        record["state"] = "prepared".into();
        record["version"] = 1.into();
        record["fence"] = 0.into();
        record["ownerId"] = Value::Null;
        record["leaseExpiresAt"] = 0.into();
        record["cancelRequested"] = false.into();
        record["providerJobId"] = Value::Null;
        record["receipt"] = Value::Null;
        record["dispatchAttempted"] = false.into();
        record["outputAssetRevisionIds"] = json!([]);
        record["resultReceipt"] = Value::Null;
        record["settlement"] = Value::Null;
        record["createdAt"] = now.into();
        record["updatedAt"] = now.into();
        tx.execute(
            "INSERT INTO engine_media_operations VALUES (?1,?2,?3,?4,?5)",
            params![
                id,
                grant_id,
                digest,
                operation["idempotencyKey"].as_str(),
                record.to_string()
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn grant() -> Value {
        json!({"id":"g","projectId":"p","scopeId":"s","branchId":"b","workTaskId":"w","maxRequests":2,"unitLimits":{"images":2},"maxCostMinor":100,"currency":"USD","expiresAt":100000})
    }
    fn operation(id: &str) -> Value {
        let request = json!({"prompt":"test"});
        json!({"id":id,"projectId":"p","scopeId":"s","branchId":"b","workTaskId":"w","attemptId":"a","providerId":"service","kind":"image.generate","idempotencyKey":id,"requestDigest":crate::digest::digest(&request),"request":request,"reservation":{"units":{"images":1},"costMinor":50,"currency":"USD"},"connection":{"credentialRealmId":"account","baseUrl":"https://api.example/v1"}})
    }
    fn apply(ledger: &mut MediaLedger, method: &str, mut input: Value) -> Result<Value> {
        input["now"] = 100.into();
        ledger.apply(method, &input)
    }
    fn change(ledger: &mut MediaLedger, record: &Value, state: &str) -> Result<Value> {
        if record["state"] == "prepared" && state == "submitting" {
            let admission = admit_at(ledger, record, 100)?;
            if admission["admitted"] != true {
                return Err("resource busy".into());
            }
        }
        apply(
            ledger,
            "transition",
            json!({"id":record["id"],"ownerId":record["ownerId"],"fence":record["fence"],"expectedVersion":record["version"],"state":state,"patch":{}}),
        )
    }
    fn admit_at(ledger: &mut MediaLedger, record: &Value, now: u64) -> Result<Value> {
        ledger.apply("admit", &json!({"id":record["id"],"ownerId":record["ownerId"],"fence":record["fence"],"globalLimit":2,"poolLimit":1,"now":now}))
    }
    fn prepare_scoped(ledger: &mut MediaLedger, id: &str, realm: &str) {
        let mut g = grant();
        g["id"] = format!("grant-{id}").into();
        g["scopeId"] = id.into();
        let mut op = operation(id);
        op["scopeId"] = id.into();
        op["connection"]["credentialRealmId"] = realm.into();
        apply(ledger, "prepare", json!({"grant":g,"operation":op})).unwrap();
    }
    fn claim_at(ledger: &mut MediaLedger, id: &str, now: u64, lease: u64) -> Value {
        ledger
            .apply(
                "claim",
                &json!({"id":id,"ownerId":format!("host-{id}"),"leaseMs":lease,"now":now}),
            )
            .unwrap()
    }
    #[test]
    fn remote_unknown_occupancy_survives_host_crash_and_lease_expiry() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("budget.sqlite");
        let mut first = MediaLedger::new(&path).unwrap();
        let mut second = MediaLedger::new(&path).unwrap();
        prepare_scoped(&mut first, "a", "account-a");
        prepare_scoped(&mut second, "b", "account-b");
        prepare_scoped(&mut second, "c", "account-c");
        for (ledger, id) in [(&mut first, "a"), (&mut second, "b")] {
            let owned = claim_at(ledger, id, 100, 20);
            let submitting = change(ledger, &owned, "submitting").unwrap();
            change(ledger, &submitting, "outcome_unknown").unwrap();
        }
        drop(first); // OS lock goes away; remote job occupancy must remain.
        drop(second);
        let mut third = MediaLedger::new(&path).unwrap();
        let waiting = claim_at(&mut third, "c", 5000, 1000);
        assert_eq!(
            admit_at(&mut third, &waiting, 5000).unwrap()["admitted"],
            false
        );
        let mut recovery = MediaLedger::new(&path).unwrap();
        let recovered = claim_at(&mut recovery, "a", 5000, 1000);
        assert_eq!(
            admit_at(&mut recovery, &recovered, 5000).unwrap()["admitted"],
            true
        );
        let release=recovery.apply("releaseAdmission",&json!({"id":"a","ownerId":recovered["ownerId"],"fence":recovered["fence"],"now":5000})).unwrap();
        assert_eq!(release["heldForRemote"], true);
        assert_eq!(
            admit_at(&mut third, &waiting, 5000).unwrap()["admitted"],
            false
        );
        recovery.apply("transition",&json!({"id":"a","ownerId":recovered["ownerId"],"fence":recovered["fence"],"expectedVersion":recovered["version"],"state":"cancelled","patch":{},"now":5000})).unwrap();
        assert_eq!(
            admit_at(&mut third, &waiting, 5000).unwrap()["admitted"],
            true
        );
    }
    #[test]
    fn account_pool_is_shared_across_work_scopes_and_prepared_dead_lease_is_reclaimable() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("budget.sqlite");
        let mut first = MediaLedger::new(&path).unwrap();
        let mut second = MediaLedger::new(&path).unwrap();
        prepare_scoped(&mut first, "a", "same-account");
        prepare_scoped(&mut second, "b", "same-account");
        let owned = claim_at(&mut first, "a", 100, 100);
        assert!(first.apply("transition",&json!({"id":"a","ownerId":owned["ownerId"],"fence":owned["fence"],"expectedVersion":owned["version"],"state":"submitting","patch":{},"now":100})).is_err());
        assert_eq!(admit_at(&mut first, &owned, 100).unwrap()["admitted"], true);
        let waiting = claim_at(&mut second, "b", 100, 1000);
        assert_eq!(
            admit_at(&mut second, &waiting, 150).unwrap()["admitted"],
            false
        );
        first.apply("renew",&json!({"id":"a","ownerId":owned["ownerId"],"fence":owned["fence"],"expectedVersion":owned["version"],"leaseMs":100,"now":150})).unwrap();
        drop(first);
        assert_eq!(
            admit_at(&mut second, &waiting, 201).unwrap()["admitted"],
            false
        );
        assert_eq!(
            admit_at(&mut second, &waiting, 251).unwrap()["admitted"],
            true
        );
        assert!(second.apply("admit",&json!({"id":"b","ownerId":waiting["ownerId"],"fence":waiting["fence"],"globalLimit":3,"poolLimit":2,"now":251})).unwrap_err().contains("policy mismatch"));
    }
    #[test]
    fn admission_and_final_dispatch_recheck_expiry_and_cancellation() {
        let temp = tempfile::tempdir().unwrap();
        let mut ledger = MediaLedger::new(&temp.path().join("budget.sqlite")).unwrap();
        prepare_scoped(&mut ledger, "a", "account");
        let owned = claim_at(&mut ledger, "a", 100, 200000);
        assert_eq!(
            admit_at(&mut ledger, &owned, 100).unwrap()["admitted"],
            true
        );
        assert!(
            admit_at(&mut ledger, &owned, 100000)
                .unwrap_err()
                .contains("expired")
        );
        assert!(ledger.apply("transition",&json!({"id":"a","ownerId":owned["ownerId"],"fence":owned["fence"],"expectedVersion":owned["version"],"state":"submitting","patch":{},"now":100000})).unwrap_err().contains("expired"));
        let cancelled = apply(&mut ledger, "cancel", json!({"id":"a"})).unwrap();
        assert!(
            admit_at(&mut ledger, &cancelled, 101)
                .unwrap_err()
                .contains("cancelled")
        );
        assert!(change(&mut ledger, &cancelled, "submitting").is_err());
    }
    #[test]
    fn durable_identity_scope_and_owner_fences() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("budget.sqlite");
        let mut first = MediaLedger::new(&path).unwrap();
        let record = apply(
            &mut first,
            "prepare",
            json!({"grant":grant(),"operation":operation("op")}),
        )
        .unwrap();
        assert_eq!(
            apply(
                &mut first,
                "prepare",
                json!({"grant":grant(),"operation":operation("op")})
            )
            .unwrap(),
            record
        );
        let mut wrong = operation("other");
        wrong["request"]["prompt"] = "changed".into();
        assert!(
            apply(
                &mut first,
                "prepare",
                json!({"grant":grant(),"operation":wrong})
            )
            .is_err()
        );
        let owned = apply(
            &mut first,
            "claim",
            json!({"id":"op","ownerId":"first","leaseMs":1000}),
        )
        .unwrap();
        let submitted = change(&mut first, &owned, "submitting").unwrap();
        let mut second = MediaLedger::new(&path).unwrap();
        assert!(
            apply(
                &mut second,
                "claim",
                json!({"id":"op","ownerId":"second","leaseMs":1000})
            )
            .is_err()
        );
        apply(&mut first, "release", json!({"id":"op"})).unwrap();
        let recovered = apply(
            &mut second,
            "claim",
            json!({"id":"op","ownerId":"second","leaseMs":1000}),
        )
        .unwrap();
        assert_eq!(recovered["state"], "submitting");
        assert!(recovered["fence"].as_u64() > submitted["fence"].as_u64());
        assert!(change(&mut first, &submitted, "failed").is_err());
        assert!(change(&mut second, &recovered, "submitting").is_err());
    }
    #[test]
    fn uncertain_submission_retains_budget_and_cannot_claim_not_sent_refund() {
        let temp = tempfile::tempdir().unwrap();
        let mut ledger = MediaLedger::new(&temp.path().join("budget.sqlite")).unwrap();
        apply(
            &mut ledger,
            "prepare",
            json!({"grant":grant(),"operation":operation("op")}),
        )
        .unwrap();
        let owned = apply(
            &mut ledger,
            "claim",
            json!({"id":"op","ownerId":"owner","leaseMs":1000}),
        )
        .unwrap();
        let submitted = change(&mut ledger, &owned, "submitting").unwrap();
        let unknown = change(&mut ledger, &submitted, "outcome_unknown").unwrap();
        let receipt = json!({"id":"op","ownerId":"owner","fence":unknown["fence"],"expectedVersion":unknown["version"],"receiptId":"receipt","outcome":"not_sent","usage":{"units":{"images":0},"costMinor":0}});
        assert!(apply(&mut ledger, "settle", receipt.clone()).is_err());
        let mut receipt = receipt;
        receipt["outcome"] = "unknown".into();
        apply(&mut ledger, "settle", receipt).unwrap();
        apply(
            &mut ledger,
            "prepare",
            json!({"grant":grant(),"operation":operation("op2")}),
        )
        .unwrap();
        assert!(
            apply(
                &mut ledger,
                "prepare",
                json!({"grant":grant(),"operation":operation("op3")})
            )
            .is_err()
        );
        let mut changed = grant();
        changed["maxRequests"] = 100.into();
        assert!(
            apply(
                &mut ledger,
                "prepare",
                json!({"grant":changed,"operation":operation("op")})
            )
            .is_err()
        );
    }
    #[test]
    fn cancel_is_orthogonal_and_terminal_requires_durable_result() {
        let temp = tempfile::tempdir().unwrap();
        let mut ledger = MediaLedger::new(&temp.path().join("budget.sqlite")).unwrap();
        apply(
            &mut ledger,
            "prepare",
            json!({"grant":grant(),"operation":operation("op")}),
        )
        .unwrap();
        apply(
            &mut ledger,
            "claim",
            json!({"id":"op","ownerId":"owner","leaseMs":1000}),
        )
        .unwrap();
        let cancelled = apply(&mut ledger, "cancel", json!({"id":"op"})).unwrap();
        assert!(change(&mut ledger, &cancelled, "submitting").is_err());
        apply(
            &mut ledger,
            "prepare",
            json!({"grant":grant(),"operation":operation("op2")}),
        )
        .unwrap();
        let owned = apply(
            &mut ledger,
            "claim",
            json!({"id":"op2","ownerId":"owner","leaseMs":1000}),
        )
        .unwrap();
        let submitting = change(&mut ledger, &owned, "submitting").unwrap();
        let collecting = change(&mut ledger, &submitting, "collecting").unwrap();
        assert!(change(&mut ledger, &collecting, "ready").is_err());
    }
}
