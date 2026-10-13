//! Domain authority for external media operations. Provider execution and money
//! accounting remain in the Engine ledger; this table pins targets and receipts.
use super::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareMediaOperation {
    pub provider_id: String,
    pub kind: String,
    pub parameters: Value,
    pub idempotency_key: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaOperationOutputs {
    pub asset_revision_ids: Vec<String>,
    pub artifact_revision_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaOperationSeal {
    pub state: String,
    pub outputs: Option<MediaOperationOutputs>,
    pub provider_receipt: Option<Value>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaTranscriptInput {
    pub text: String,
    pub source_asset_revision_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MediaBudget {
    max_requests: u64,
    unit_limits: BTreeMap<String, u64>,
    max_cost_minor: Option<u64>,
    currency: Option<String>,
    expires_at: i64,
}
fn work(db: &Connection, task: &crate::TaskRecord) -> Result<String, RepositoryError> {
    db.query_row("SELECT s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND w.project_id=?2",params![task.task_id,task.project_id.as_str()],|r|r.get(0)).optional().map_err(storage)?.ok_or(RepositoryError::TaskBindingInvalid)
}
fn load(db: &Connection, p: &str, op: &str, now_ms: i64) -> Result<Value, RepositoryError> {
    let row=db.query_row("SELECT task_id,operation_json,grant_json,state,seal_json,expected_output_count FROM story_media_operation_intents_v1 WHERE project_id=?1 AND operation_id=?2",params![p,op],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,Option<i64>>(5)?))).optional().map_err(storage)?.ok_or_else(||fail("media_operation_not_found"))?;
    let operation: Value = decode(&row.1)?;
    let grant: Value = decode(&row.2)?;
    let task = crate::tasks::load_task(db, &row.0)?;
    let e = edition(
        db,
        p,
        operation["editionId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?,
        operation["branchId"].as_str(),
    )?;
    let active = assert_task_media_write(db,&task)
        .and_then(|_| assert_task_scope(db, &task, &e.id, Some(&e.branch_id)))
        .and_then(|_| crate::task_input::assert_task_inputs_current(db, &task))
        .is_ok()
        && task.capability_expires_at > now_ms / 1000
        && grant["expiresAt"].as_i64().is_some_and(|at| at > now_ms);
    let status: Option<String> = db
        .query_row(
            "SELECT status FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2",
            params![operation["workTaskId"].as_str(), p],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let source_unchanged =
        member(db, &head(db, &e)?, &e.id)?.as_deref() == operation["source"]["revisionId"].as_str();
    let can_dispatch =
        active && source_unchanged && status.as_deref() == Some("running") && row.3 == "prepared";
    Ok(
        json!({"operation":operation,"grant":grant,"task":{"id":task.task_id,"projectId":task.project_id.as_str(),"threadId":task.thread_id,"operation":task.operation,"status":task.status,"inputRevision":task.input_revision,"target":decode::<Value>(&task.target_json)?},"state":row.3,"expectedOutputCount":row.5,"authorization":{"canDispatch":can_dispatch,"canReconcile":true,"reason":if can_dispatch {Value::Null}else{json!("任务已暂停、来源已变化或授权已到期；只能核对已提交作业。")}},"seal":row.4.map(|s|decode::<Value>(&s)).transpose()?}),
    )
}
pub(super) fn allowed_input_assets(
    db: &Connection,
    task: &crate::TaskRecord,
    e: &CreativeEdition,
) -> Result<BTreeSet<String>, RepositoryError> {
    let frozen: String = db
        .query_row(
            "SELECT asset_revisions_json FROM story_media_task_bindings_v1 WHERE task_id=?1",
            [&task.task_id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let mut result: BTreeSet<String> = decode::<Vec<String>>(&frozen)?.into_iter().collect();
    let work_id = work(db, task)?;
    let mut q=db.prepare("SELECT i.asset_revision_id FROM story_media_imports_v1 i JOIN story_media_operation_intents_v1 o ON i.operation_id GLOB (o.operation_id || '-output-[0-9]*') WHERE o.work_task_id=?1 AND o.scope_id=?2 AND o.branch_id=?3 AND o.state='ready' AND i.status='registered'").map_err(storage)?;
    for r in q
        .query_map(params![work_id, e.scope_id, e.branch_id], |r| {
            r.get::<_, String>(0)
        })
        .map_err(storage)?
    {
        result.insert(r.map_err(storage)?);
    }
    Ok(result)
}
fn input_ids(parameters: &Value) -> Result<Vec<String>, RepositoryError> {
    let mut ids = vec![];
    for name in [
        "imageAssetRevisionId",
        "maskAssetRevisionId",
        "audioAssetRevisionId",
    ] {
        if let Some(v) = parameters.get(name) {
            ids.push(
                v.as_str()
                    .ok_or_else(|| fail("media_input_invalid"))?
                    .to_owned(),
            );
        }
    }
    if let Some(v) = parameters.get("imageAssetRevisionIds") {
        for id in v.as_array().ok_or_else(|| fail("media_input_invalid"))? {
            ids.push(
                id.as_str()
                    .ok_or_else(|| fail("media_input_invalid"))?
                    .to_owned(),
            );
        }
    }
    if ids.len() > 17 {
        return Err(fail("media_input_limit"));
    }
    Ok(ids)
}
fn reservation(
    db: &Connection,
    p: &str,
    request: &PrepareMediaOperation,
    currency: &Option<String>,
) -> Result<Value, RepositoryError> {
    let (unit, amount) = match request.kind.as_str() {
        "image.generate" | "image.edit" => ("images", 1),
        "video.generate" => (
            "videoMilliseconds",
            request.parameters["duration"]
                .as_u64()
                .filter(|n| *n > 0 && *n <= 3600)
                .and_then(|n| n.checked_mul(1000))
                .ok_or_else(|| fail("media_duration_invalid"))?,
        ),
        "audio.speech" => (
            "characters",
            request.parameters["text"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 100_000)
                .ok_or_else(|| fail("media_text_invalid"))?
                .encode_utf16()
                .count() as u64,
        ),
        "audio.transcribe" => {
            let asset = request.parameters["audioAssetRevisionId"]
                .as_str()
                .ok_or_else(|| fail("media_input_invalid"))?;
            let stored:String=db.query_row("SELECT metadata_json FROM story_media_assets_v1 WHERE project_id=?1 AND asset_revision_id=?2",params![p,asset],|r|r.get(0)).map_err(storage)?;
            let a: MediaAsset = decode(&stored)?;
            let t = a.duration.ok_or_else(|| fail("media_duration_invalid"))?;
            validate::time(&t, true)?;
            let n = (t.value as u128)
                .checked_mul(t.rate.denominator as u128)
                .and_then(|v| v.checked_mul(1000))
                .ok_or_else(|| fail("media_duration_invalid"))?;
            let ms = n.div_ceil(t.rate.numerator as u128);
            (
                "audioMilliseconds",
                u64::try_from(ms).map_err(|_| fail("media_duration_invalid"))?,
            )
        }
        "local.render" => return Err(fail("media_use_frozen_export")),
        _ => return Err(fail("media_operation_kind_invalid")),
    };
    Ok(json!({"units":{unit:amount},"costMinor":null,"currency":currency}))
}

pub(crate) fn assert_no_pending_operations(
    db: &Connection,
    task: &str,
) -> Result<(), RepositoryError> {
    let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_media_operation_intents_v1 o JOIN story_work_steps_v1 s ON s.work_task_id=o.work_task_id JOIN story_work_attempts_v1 a ON a.step_id=s.id WHERE a.attempt_id=?1 AND o.state='prepared')",[task],|r|r.get(0)).map_err(storage)?;
    if pending {
        return Err(fail("media_operation_pending"));
    }
    Ok(())
}

/// Public multimedia operations capability; SQLite retains transaction ownership.
pub trait MultimediaOperationsRepository {
fn media_transcript_page(
        &self,
        p: &ProjectId,
        op: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Value, RepositoryError>;
fn prepare_media_operation(
        &mut self,
        task: &crate::TaskRecord,
        r: &PrepareMediaOperation,
        now_ms: i64,
    ) -> Result<Value, RepositoryError>;
/// Read-only reconciliation is valid after capability expiry; it never grants dispatch.
    fn media_operation_intent(
        &self,
        p: &ProjectId,
        operation: &str,
        now_ms: i64,
    ) -> Result<Value, RepositoryError>;
fn media_task_operation(
        &self,
        task: &crate::TaskRecord,
        operation: &str,
        now_ms: i64,
    ) -> Result<Value, RepositoryError>;
/// Pins the complete output manifest before any one output is archived.
    fn begin_media_operation_outputs(
        &mut self,
        p: &ProjectId,
        op: &str,
        count: usize,
        now_ms: i64,
    ) -> Result<Value, RepositoryError>;
fn store_media_transcript(
        &mut self,
        p: &ProjectId,
        op: &str,
        r: &MediaTranscriptInput,
        now_ms: i64,
    ) -> Result<Value, RepositoryError>;
/// A complete receipt may arrive after cancellation; it is archived in its
    /// original scope, never applied automatically to the document's new head.
    fn seal_media_operation(
        &mut self,
        p: &ProjectId,
        op: &str,
        r: &MediaOperationSeal,
        now_ms: i64,
    ) -> Result<Value, RepositoryError>;
}

impl MultimediaOperationsRepository for SqliteStoryRepository {
fn media_transcript_page(
        &self,
        p: &ProjectId,
        op: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Value, RepositoryError> {
        if limit == 0 || limit > 8000 {
            return Err(fail("media_page_limit_invalid"));
        }
        let stored:Option<(String,String)>=self.connection.query_row("SELECT r.revision_id,r.content_json FROM story_media_operation_intents_v1 o JOIN story_revisions_v2 r ON r.revision_id=o.transcript_revision_id WHERE o.project_id=?1 AND o.operation_id=?2 AND o.state='ready'",params![p.as_str(),op],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage)?;
        let (revision, json) = stored.ok_or_else(|| fail("media_transcript_not_found"))?;
        let doc: Value = decode(&json)?;
        let text = doc["text"].as_str().ok_or(RepositoryError::CorruptState)?;
        let total = text.chars().count();
        if offset > total {
            return Err(fail("media_page_offset_invalid"));
        }
        let page: String = text.chars().skip(offset).take(limit).collect();
        let next = offset + page.chars().count();
        Ok(
            json!({"artifactRevisionId":revision,"sourceAssetRevisionId":doc["sourceAssetRevisionId"],"offset":offset,"text":page,"nextOffset":if next<total{Some(next)}else{None},"totalChars":total}),
        )
    }

fn prepare_media_operation(
        &mut self,
        task: &crate::TaskRecord,
        r: &PrepareMediaOperation,
        now_ms: i64,
    ) -> Result<Value, RepositoryError> {
        validate::key(&r.provider_id)?;
        validate::key(&r.idempotency_key)?;
        if !r.parameters.is_object() || encode(&r.parameters)?.len() > 512 * 1024 {
            return Err(fail("media_parameters_invalid"));
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let target: Value = decode(&task.target_json)?;
        let e = media_target(&tx, task.project_id.as_str(), &target)?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        assert_task_media_write(&tx,task)?;
        assert_task_scope(&tx, task, &e.id, Some(&e.branch_id))?;
        crate::task_input::assert_task_inputs_current(&tx, task)?;
        crate::task_intents::assert_no_pending_intent(&tx, &task.task_id)?;
        let work_id = work(&tx, task)?;
        let key_hash = digest(&json!([
            task.project_id.as_str(),
            work_id,
            r.idempotency_key
        ]))?;
        let op = format!("media-{key_hash}");
        let hash = digest(&json!([e.id, e.scope_id, e.branch_id, r]))?;
        let stored: Option<String> = tx
            .query_row(
                "SELECT request_digest FROM story_media_operation_intents_v1 WHERE operation_id=?1",
                [&op],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        if let Some(stored) = stored {
            if stored != hash {
                return Err(RepositoryError::IdempotencyConflict);
            }
            let result = load(&tx, task.project_id.as_str(), &op, now_ms)?;
            tx.commit().map_err(storage)?;
            return Ok(result);
        }
        let (authorization,root_target):(Option<String>,String)=tx.query_row("SELECT b.initial_authorization_id,t.target_json FROM story_work_budget_authorizations_v1 b JOIN story_work_steps_v1 s ON s.work_task_id=b.work_task_id JOIN story_work_attempts_v1 a ON a.step_id=s.id JOIN story_tasks_v2 t ON t.task_id=a.attempt_id WHERE b.work_task_id=?1 ORDER BY a.sequence LIMIT 1",[&work_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage)?.ok_or_else(||fail("media_budget_required"))?;
        let auth = authorization.ok_or_else(|| fail("media_budget_required"))?;
        let root: Value = decode(&root_target)?;
        if (root["editionId"] != e.id
            || root["scopeId"] != e.scope_id
            || root["branchId"] != e.branch_id)
            && !adaptation::work_owns_edition(&tx, &work_id, &e)?
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let budget: MediaBudget = serde_json::from_value(root["mediaBudget"].clone())
            .map_err(|_| fail("media_budget_required"))?;
        if budget.max_requests == 0
            || budget.max_requests > 10000
            || budget.expires_at <= now_ms
            || budget.unit_limits.is_empty()
            || budget.unit_limits.len() > 8
            || budget.unit_limits.iter().any(|(k, v)| {
                !matches!(
                    k.as_str(),
                    "images"
                        | "videoMilliseconds"
                        | "audioMilliseconds"
                        | "characters"
                        | "renderFrames"
                        | "bytes"
                ) || *v == 0
            })
        {
            return Err(fail("media_budget_invalid"));
        }
        if budget.max_cost_minor.is_some() {
            return Err(fail("media_cost_quote_required"));
        }
        let allowed = allowed_input_assets(&tx, task, &e)?;
        let inputs = input_ids(&r.parameters)?;
        if inputs.iter().any(|id| !allowed.contains(id)) {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        let ws = workspace(&tx, task.project_id.as_str(), &e.id, Some(&e.branch_id))?;
        let selector: MediaSelector = match target.get("selection").filter(|v| !v.is_null()) {
            Some(selection) => serde_json::from_value(selection["selector"].clone())
                .map_err(|_| fail("media_selector_invalid"))?,
            None => MediaSelector::Whole,
        };
        select_document_source(&tx, task.project_id.as_str(), &ws.document, &selector)?;
        let reservation = reservation(&tx, task.project_id.as_str(), r, &budget.currency)?;
        let source = json!({"snapshotId":ws.head_snapshot_id,"revisionId":ws.revision_id,"sourceDigest":ws.source_digest,"assetRevisionIds":inputs});
        let operation = json!({"id":op,"projectId":task.project_id.as_str(),"scopeId":e.scope_id,"branchId":e.branch_id,"workTaskId":work_id,"attemptId":task.task_id,"editionId":e.id,"providerId":r.provider_id,"kind":r.kind,"idempotencyKey":r.idempotency_key,"request":r.parameters,"requestDigest":digest(&r.parameters)?,"reservation":reservation,"source":source,"target":selector});
        let mut grant = serde_json::to_value(budget).map_err(|_| RepositoryError::CorruptState)?;
        for (k, v) in [
            ("id", json!(auth)),
            ("projectId", json!(task.project_id.as_str())),
            ("scopeId", json!(e.scope_id)),
            ("branchId", json!(e.branch_id)),
            ("workTaskId", json!(work_id)),
        ] {
            grant[k] = v;
        }
        tx.execute("INSERT INTO story_media_operation_intents_v1(operation_id,project_id,task_id,work_task_id,edition_id,scope_id,branch_id,request_digest,operation_json,grant_json,state,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'prepared',?11)",params![op,task.project_id.as_str(),task.task_id,work_id,e.id,e.scope_id,e.branch_id,hash,encode(&operation)?,encode(&grant)?,now_ms]).map_err(storage)?;
        let result = load(&tx, task.project_id.as_str(), &op, now_ms)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }

/// Read-only reconciliation is valid after capability expiry; it never grants dispatch.
    fn media_operation_intent(
        &self,
        p: &ProjectId,
        operation: &str,
        now_ms: i64,
    ) -> Result<Value, RepositoryError> {
        load(&self.connection, p.as_str(), operation, now_ms)
    }

fn media_task_operation(
        &self,
        task: &crate::TaskRecord,
        operation: &str,
        now_ms: i64,
    ) -> Result<Value, RepositoryError> {
        let e = self.media_task_scope(task)?;
        let result = load(
            &self.connection,
            task.project_id.as_str(),
            operation,
            now_ms,
        )?;
        if result["operation"]["workTaskId"] != work(&self.connection, task)?
            || result["operation"]["scopeId"] != e.scope_id
            || result["operation"]["branchId"] != e.branch_id
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        Ok(result)
    }

/// Pins the complete output manifest before any one output is archived.
    fn begin_media_operation_outputs(
        &mut self,
        p: &ProjectId,
        op: &str,
        count: usize,
        now_ms: i64,
    ) -> Result<Value, RepositoryError> {
        if !(1..=16).contains(&count) {
            return Err(fail("media_output_count_invalid"));
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let intent = load(&tx, p.as_str(), op, now_ms)?;
        if let Some(existing) = intent["expectedOutputCount"].as_u64() {
            if existing != count as u64 {
                return Err(RepositoryError::IdempotencyConflict);
            }
            if intent["state"] == "prepared" {
                tx.execute("UPDATE story_media_imports_v1 SET lease_expires_at=?1 WHERE operation_id GLOB ?2 AND status='receiving'",params![now_ms/1000+86400,format!("{op}-output-[0-9]*")]).map_err(storage)?;
            }
            tx.commit().map_err(storage)?;
            return Ok(json!({"outputCount":count}));
        }
        if intent["state"] != "prepared" {
            return Err(fail("media_operation_terminal"));
        }
        let kind = intent["operation"]["kind"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        if kind != "video.generate" && count != 1 || kind == "video.generate" && count > 4 {
            return Err(fail("media_output_count_invalid"));
        }
        if kind != "audio.transcribe" {
            for i in 0..count {
                let pin = format!("{op}-output-{i}");
                let name = format!("generated-{i}");
                tx.execute(
                    "INSERT INTO story_media_imports_v1 VALUES(?1,?2,?3,?4,?5,'receiving',?6,NULL)",
                    params![
                        pin,
                        p.as_str(),
                        intent["operation"]["scopeId"].as_str(),
                        name,
                        digest(&json!([op, count, i]))?,
                        now_ms / 1000 + 86400
                    ],
                )
                .map_err(storage)?;
            }
        }
        tx.execute("UPDATE story_media_operation_intents_v1 SET expected_output_count=?1 WHERE operation_id=?2",params![count as i64,op]).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(json!({"outputCount":count}))
    }

fn store_media_transcript(
        &mut self,
        p: &ProjectId,
        op: &str,
        r: &MediaTranscriptInput,
        now_ms: i64,
    ) -> Result<Value, RepositoryError> {
        validate::text(&r.text, 1_000_000, false)?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let intent = load(&tx, p.as_str(), op, now_ms)?;
        if intent["operation"]["kind"] != "audio.transcribe"
            || intent["operation"]["request"]["audioAssetRevisionId"] != r.source_asset_revision_id
            || intent["expectedOutputCount"] != 1
        {
            return Err(fail("media_transcript_binding_mismatch"));
        }
        let content = json!({"kind":"mediaTranscript","text":r.text,"sourceAssetRevisionId":r.source_asset_revision_id,"operationId":op});
        let hash = digest(&content)?;
        let stored:Option<String>=tx.query_row("SELECT transcript_revision_id FROM story_media_operation_intents_v1 WHERE operation_id=?1",[op],|r|r.get(0)).map_err(storage)?;
        if let Some(revision) = stored {
            let prior: String = tx
                .query_row(
                    "SELECT content_digest FROM story_revisions_v2 WHERE revision_id=?1",
                    [&revision],
                    |r| r.get(0),
                )
                .map_err(storage)?;
            if prior != hash {
                return Err(RepositoryError::IdempotencyConflict);
            }
            return Ok(json!({"artifactRevisionId":revision}));
        }
        if intent["state"] != "prepared" {
            return Err(fail("media_operation_terminal"));
        }
        let revision = id("revision");
        let object = id("transcript");
        tx.execute(
            "INSERT INTO story_revisions_v2 VALUES(?1,?2,?3,'mediaTranscript',NULL,?4,?5)",
            params![revision, p.as_str(), object, hash, encode(&content)?],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO story_media_revision_scopes_v1 VALUES(?1,?2,?3)",
            params![
                revision,
                p.as_str(),
                intent["operation"]["scopeId"].as_str()
            ],
        )
        .map_err(storage)?;
        tx.execute("UPDATE story_media_operation_intents_v1 SET transcript_revision_id=?1 WHERE operation_id=?2",params![revision,op]).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(json!({"artifactRevisionId":revision}))
    }

/// A complete receipt may arrive after cancellation; it is archived in its
    /// original scope, never applied automatically to the document's new head.
    fn seal_media_operation(
        &mut self,
        p: &ProjectId,
        op: &str,
        r: &MediaOperationSeal,
        now_ms: i64,
    ) -> Result<Value, RepositoryError> {
        if !matches!(r.state.as_str(), "ready" | "failed" | "cancelled") {
            return Err(fail("media_operation_state_invalid"));
        }
        if r.error.as_ref().is_some_and(|s| s.len() > 4000)
            || r.provider_receipt
                .as_ref()
                .is_some_and(|v| v.to_string().len() > 64 * 1024)
        {
            return Err(fail("media_receipt_invalid"));
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let intent = load(&tx, p.as_str(), op, now_ms)?;
        let seal = json!({"operationId":op,"state":r.state,"outputs":r.outputs,"providerReceipt":r.provider_receipt,"error":r.error});
        if !intent["seal"].is_null() {
            if intent["seal"] != seal {
                return Err(RepositoryError::IdempotencyConflict);
            }
            return Ok(seal);
        }
        if r.state == "ready" {
            let outputs = r
                .outputs
                .as_ref()
                .ok_or_else(|| fail("media_outputs_missing"))?;
            let count = intent["expectedOutputCount"]
                .as_u64()
                .ok_or_else(|| fail("media_output_manifest_missing"))?
                as usize;
            if intent["operation"]["kind"] == "audio.transcribe" {
                let revision:Option<String>=tx.query_row("SELECT transcript_revision_id FROM story_media_operation_intents_v1 WHERE operation_id=?1",[op],|r|r.get(0)).map_err(storage)?;
                if count != 1
                    || revision.is_none()
                    || revision != outputs.artifact_revision_id
                    || !outputs.asset_revision_ids.is_empty()
                {
                    return Err(fail("media_outputs_incomplete"));
                }
            } else {
                if outputs.artifact_revision_id.is_some()
                    || outputs.asset_revision_ids.len() != count
                {
                    return Err(fail("media_outputs_incomplete"));
                }
                let mut actual = BTreeSet::new();
                for i in 0..count {
                    let revision:Option<String>=tx.query_row("SELECT asset_revision_id FROM story_media_imports_v1 WHERE operation_id=?1 AND project_id=?2 AND scope_id=?3 AND status='registered'",params![format!("{op}-output-{i}"),p.as_str(),intent["operation"]["scopeId"].as_str()],|r|r.get(0)).optional().map_err(storage)?.flatten();
                    let revision = revision.ok_or_else(|| fail("media_outputs_incomplete"))?;
                    let metadata:String=tx.query_row("SELECT metadata_json FROM story_media_assets_v1 WHERE asset_revision_id=?1",[&revision],|r|r.get(0)).map_err(storage)?;
                    let metadata: MediaAsset = decode(&metadata)?;
                    let expected = match intent["operation"]["kind"].as_str() {
                        Some("image.generate" | "image.edit") => MediaAssetKind::Image,
                        Some("video.generate") => MediaAssetKind::Video,
                        Some("audio.speech") => MediaAssetKind::Audio,
                        _ => return Err(fail("media_output_kind_invalid")),
                    };
                    if metadata.kind != expected {
                        return Err(fail("media_output_kind_mismatch"));
                    }
                    actual.insert(revision);
                }
                if actual.len() != count
                    || actual != outputs.asset_revision_ids.iter().cloned().collect()
                {
                    return Err(fail("media_outputs_incomplete"));
                }
            }
        } else if r.outputs.is_some() {
            return Err(fail("media_terminal_outputs_invalid"));
        }
        tx.execute("UPDATE story_media_operation_intents_v1 SET state=?1,seal_json=?2 WHERE operation_id=?3",params![r.state,encode(&seal)?,op]).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(seal)
    }
}

use crate::{MultimediaRepository};
