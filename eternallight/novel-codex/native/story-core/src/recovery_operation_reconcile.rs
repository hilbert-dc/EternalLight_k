use rusqlite::params;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{ObjectId, RecoveryOperationBinding, RepositoryError, RevisionId, SqliteStoryRepository};

#[path = "../../../shared/novel-operation-digest.rs"]
mod operation_digest;
#[path = "../../../shared/draft-checkpoint-identity.rs"]
mod draft_identity;

const MAX_ARGUMENT_BYTES: usize = 128 * 1024;

impl SqliteStoryRepository {
    /// Reconcile one original candidate call from authenticated, complete request
    /// evidence. This records facts only; it cannot revive a task, grant writes,
    /// save content, settle provider usage or alter a Work's lifecycle.
    pub fn reconcile_candidate_operation(
        &mut self,
        binding: &RecoveryOperationBinding,
        arguments: &Value,
    ) -> Result<Option<Value>, RepositoryError> {
        if !arguments.is_object() || arguments.to_string().len() > MAX_ARGUMENT_BYTES {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let normalized = operation_digest::host_json(arguments)
            .map_err(|_| RepositoryError::TaskBindingInvalid)?;
        let arguments = &normalized;
        let hash = operation_digest::operation_digest(&binding.tool_name, arguments)
            .map_err(|_| RepositoryError::TaskBindingInvalid)?;
        if hash != binding.request_hash
            || arguments["projectId"] != binding.project_id.as_str()
            || arguments["taskId"] != binding.task_id
            || arguments["threadId"] != binding.thread_id
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let expected_revision = arguments["expectedRevision"].as_i64()
            .filter(|revision| *revision >= 0)
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        // Absence is not JSON null. Preserve the original argument's JSON type.
        let submitted_candidate = nullable_revision(arguments.get("expectedCandidateRevisionId"))?;
        let key = arguments["idempotencyKey"].as_str()
            .filter(|key| !key.is_empty() && key.len() <= 160 && !key.chars().any(char::is_control))
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let key_ref = format!("sha256:{:x}", Sha256::digest(json!(key).to_string().as_bytes()));

        let tx = self.connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let operation = crate::recovery_operation_read::read_operation(&tx, binding, None)?;
        let Some(operation) = operation else {
            tx.commit().map_err(storage_error)?;
            return Ok(None);
        };
        if operation["idempotency_ref"] != key_ref {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let task = crate::tasks::load_task(&tx, &binding.task_id)?;
        if task.input_revision != expected_revision {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let kind = task.required_output_kind.as_deref()
            .ok_or(RepositoryError::TaskOutputForbidden)?;
        if crate::recovery_operations::candidate_tool(kind)? != binding.tool_name {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if matches!(operation["state"].as_str(), Some("committed" | "rejected")) {
            tx.commit().map_err(storage_error)?;
            return Ok(Some(operation));
        }
        if operation["state"] != "accepted" {
            return Err(RepositoryError::CorruptState);
        }

        // An earlier idempotent save must not be contradicted. Only the exact
        // supported body-only transport can distinguish another request's pair.
        let receipt_exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_candidate_receipts_v2 WHERE project_id=?1 AND idempotency_key=?2) OR EXISTS(SELECT 1 FROM story_receipts_v2 WHERE project_id=?1 AND idempotency_key=?2)",
            params![binding.project_id.as_str(), key], |row| row.get(0),
        ).map_err(storage_error)?;
        let output_exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_task_outputs_v2 WHERE task_id=?1)",
            [&binding.task_id], |row| row.get(0),
        ).map_err(storage_error)?;
        let refs: Value = serde_json::from_str(&task.output_refs_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        let refs = refs.as_array().ok_or(RepositoryError::CorruptState)?;
        if !matches!(task.status.as_str(), "completed" | "failed" | "cancelled")
            || task.capability_revoked_at.is_none()
            || (receipt_exists && (kind != "draft" || task.status != "completed"))
        {
            tx.commit().map_err(storage_error)?;
            return Ok(Some(operation));
        }
        let manifest: Value = serde_json::from_str(&task.input_manifest_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        let object = manifest["outputObjectId"].as_str()
            .ok_or(RepositoryError::CorruptState)?;
        ObjectId::parse(object.to_owned()).map_err(|_| RepositoryError::CorruptState)?;
        let frozen_candidate = nullable_revision(manifest.get("expectedCandidateRevisionId"))
            .map_err(|_| RepositoryError::CorruptState)?;
        // The frozen CAS proof remains limited to a failed/cancelled task with
        // no output. A later output cannot prove an earlier CAS was impossible.
        let frozen_cas_rejected = !receipt_exists && task.status != "completed"
            && !output_exists && refs.is_empty() && submitted_candidate != frozen_candidate;
        let scoped_body_rejected = !frozen_cas_rejected && kind == "draft"
            && scoped_body_rejection(&tx, &task, arguments, object, frozen_candidate.as_ref())?;
        let consistent = if scoped_body_rejected {
            if task.status == "completed" {
                completed_outputs_consistent(&tx, &task, refs, object)?
            } else {
                !output_exists && refs.is_empty()
            }
        } else { false };
        let receipt_permits = !receipt_exists || (scoped_body_rejected && consistent
            && different_request_receipt(
                &tx, binding, key, &key_ref, expected_revision, object,
                &draft_identity::draft_request_digest(
                    &arguments["payload"], expected_revision, submitted_candidate.as_ref().map(RevisionId::as_str),
                ),
            )?);
        if frozen_cas_rejected || (scoped_body_rejected && consistent && receipt_permits) {
            // Both proofs concern this exact original call and immutable inputs.
            // Neither today's head, another call nor error prose proves its fate.
            crate::recovery_operations::reject_operation(
                &tx, Some(binding), &binding.project_id, &binding.task_id, &binding.tool_name,
            )?;
        }
        let result = crate::recovery_operation_read::read_operation(&tx, binding, None)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}

/// A different request's pair cannot replay this body-only request. Require all
/// receipt, commit, snapshot and own-call relations; absence is still unknown.
fn different_request_receipt(
    db: &rusqlite::Connection,
    binding: &RecoveryOperationBinding,
    key: &str,
    key_ref: &str,
    input_revision: i64,
    object: &str,
    original_digest: &str,
) -> Result<bool, RepositoryError> {
    db.query_row(
        "SELECT
          (SELECT COUNT(*) FROM story_candidate_receipts_v2 WHERE project_id=?1 AND idempotency_key=?2)=1
          AND (SELECT COUNT(*) FROM story_receipts_v2 WHERE project_id=?1 AND idempotency_key=?2)=1
          AND EXISTS(
            SELECT 1 FROM story_candidate_receipts_v2 c
            JOIN story_receipts_v2 r ON r.project_id=c.project_id AND r.branch_id=c.branch_id
              AND r.idempotency_key=c.idempotency_key AND r.request_digest=c.request_digest
              AND r.result_snapshot_id=c.result_snapshot_id AND r.status='applied'
            JOIN story_commits_v2 commit_row ON commit_row.commit_id=r.commit_id
              AND commit_row.project_id=c.project_id AND commit_row.branch_id='main'
              AND commit_row.result_snapshot_id=c.result_snapshot_id AND commit_row.status='applied'
            JOIN story_snapshots_v2 s ON s.snapshot_id=c.result_snapshot_id
              AND s.project_id=c.project_id AND s.branch_id='main'
              AND s.parent_snapshot_id=commit_row.base_snapshot_id
            JOIN story_snapshot_members_v2 member ON member.snapshot_id=s.snapshot_id
              AND member.object_id=?10 AND member.revision_id=c.revision_id
            JOIN story_task_outputs_v2 own ON own.task_id=?5
              AND own.output_kind='draft' AND own.revision_id=c.revision_id
            JOIN story_tool_operations_v1 other ON other.project_id=c.project_id
              AND other.task_id=?5 AND other.thread_id=?6
              AND other.tool_name='save_draft_checkpoint' AND other.state='committed'
              AND other.idempotency_ref=?9 AND other.call_id<>?7 AND other.request_hash<>?8
              AND other.receipt_ref='runtime:candidate:' || c.revision_id
            WHERE c.project_id=?1 AND c.idempotency_key=?2 AND c.branch_id='main'
              AND c.project_revision=?3 AND c.request_digest<>?4
              AND length(c.request_digest)=64 AND c.request_digest NOT GLOB '*[^0-9a-f]*'
          )",
        params![binding.project_id.as_str(),key,input_revision,original_digest,
            binding.task_id,binding.thread_id,binding.call_id,binding.request_hash,key_ref,object],
        |row| row.get(0),
    ).map_err(storage_error)
}

/// This call's body cannot satisfy selectedScenes body alignment, independent
/// of every later checkpoint. Body-only transport preserves the raw body and
/// creates one group from it; neither this scope nor this call merges a prefix.
fn scoped_body_rejection(
    db: &rusqlite::Connection,
    task: &crate::TaskRecord,
    arguments: &Value,
    object: &str,
    frozen_candidate: Option<&RevisionId>,
) -> Result<bool, RepositoryError> {
    if task.operation != "generateChapter" || arguments.get("delegatedCandidateId").is_some() {
        return Ok(false);
    }
    let target: Value = serde_json::from_str(&task.target_json)
        .map_err(|_| RepositoryError::CorruptState)?;
    if target["scope"]["type"] != "selectedScenes"
        || crate::task_scope::writer_scene_limit(&target)?.is_some()
    {
        return Ok(false);
    }
    let Some(payload) = arguments["payload"].as_object() else { return Ok(false) };
    let Some(body) = payload.get("body").and_then(Value::as_str) else { return Ok(false) };
    if payload.contains_key("sceneGroups")
        || payload.get("chapterId") != target.get("chapterId")
        || payload.get("chapterNumber") != target.get("chapterNumber")
        || payload.get("chapterNumber").and_then(Value::as_i64).is_none_or(|n| n < 1)
        || payload.get("title").and_then(Value::as_str).is_none_or(|s| s.chars().count() > 160)
        || payload.get("completeness").and_then(Value::as_str).is_none()
        || body.chars().count() > 120_000
        || body == crate::scene_body::canonical_scene_body([body])
    {
        return Ok(false);
    }
    crate::scene_revision::frozen_scoped_base(
        db, task, &target, object, frozen_candidate.map(RevisionId::as_str),
    )?;
    Ok(true)
}

/// Check only immutable own-output relations, never a current manuscript head.
/// A completed task with absent, foreign or contradictory output stays unknown.
fn completed_outputs_consistent(
    db: &rusqlite::Connection,
    task: &crate::TaskRecord,
    refs: &[Value],
    object: &str,
) -> Result<bool, RepositoryError> {
    if refs.is_empty() || refs.len() > 10_000 { return Ok(false) }
    let target: Value = serde_json::from_str(&task.target_json)
        .map_err(|_| RepositoryError::CorruptState)?;
    let mut statement = db.prepare(
        "SELECT o.ordinal,o.output_kind,o.revision_id,o.working_base_revision_id,o.completeness,
                r.project_id,r.object_id,r.object_kind,r.parent_revision_id,r.content_json
         FROM story_task_outputs_v2 o LEFT JOIN story_revisions_v2 r ON r.revision_id=o.revision_id
         WHERE o.task_id=?1 ORDER BY o.ordinal",
    ).map_err(storage_error)?;
    let rows = statement.query_map([&task.task_id], |row| Ok((
        row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
        row.get::<_, Option<String>>(3)?, row.get::<_, Option<String>>(4)?,
        row.get::<_, Option<String>>(5)?, row.get::<_, Option<String>>(6)?,
        row.get::<_, Option<String>>(7)?, row.get::<_, Option<String>>(8)?,
        row.get::<_, Option<String>>(9)?,
    ))).map_err(storage_error)?;
    let mut count = 0;
    let mut parent = Some(target["scope"]["sourceRevisionId"].as_str()
        .ok_or(RepositoryError::CorruptState)?.to_owned());
    let mut last_complete = false;
    for row in rows {
        let (ordinal, kind, revision, base, completeness, project, own_object, own_kind, own_parent, content)
            = row.map_err(storage_error)?;
        let mut reference = json!({"kind":kind,"revisionId":revision,"objectId":object});
        if let Some(base) = base.as_deref() {
            reference["workingBaseRevisionId"] = json!(base);
            let valid_base: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM story_revisions_v2 WHERE revision_id=?1 AND project_id=?2 AND object_id=?3 AND object_kind='draft')",
                params![base,task.project_id.as_str(),object], |row| row.get(0),
            ).map_err(storage_error)?;
            if !valid_base { return Ok(false) }
        }
        if ordinal != count as i64 + 1 || refs.get(count) != Some(&reference)
            || kind != "draft" || project.as_deref() != Some(task.project_id.as_str())
            || own_object.as_deref() != Some(object) || own_kind.as_deref() != Some("draft")
            || own_parent != parent
        {
            return Ok(false);
        }
        let Some(content) = content else { return Ok(false) };
        let content_value: Value = serde_json::from_str(&content)
            .map_err(|_| RepositoryError::CorruptState)?;
        if content_value["completeness"].as_str() != completeness.as_deref() {
            return Ok(false);
        }
        match crate::scene_revision::validate_write(db, task, &target, &content) {
            Ok(()) => {}
            Err(RepositoryError::Storage(error)) => return Err(RepositoryError::Storage(error)),
            Err(_) => return Ok(false),
        }
        last_complete = completeness.as_deref() == Some("chapterComplete");
        parent = Some(revision);
        count += 1;
    }
    Ok(count == refs.len() && last_complete)
}

fn nullable_revision(value: Option<&Value>) -> Result<Option<RevisionId>, RepositoryError> {
    match value {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => RevisionId::parse(value.clone()).map(Some)
            .map_err(|_| RepositoryError::TaskBindingInvalid),
        _ => Err(RepositoryError::TaskBindingInvalid),
    }
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
