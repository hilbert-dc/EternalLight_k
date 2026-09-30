use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_tool_operations_v1 (
 operation_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id),
 thread_id TEXT NOT NULL, turn_id TEXT NOT NULL, call_id TEXT NOT NULL, tool_name TEXT NOT NULL,
 request_hash TEXT NOT NULL, idempotency_ref TEXT, state TEXT NOT NULL CHECK(state IN ('accepted','committed','rejected')),
 receipt_ref TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 UNIQUE(project_id, task_id, call_id), CHECK((state = 'committed') = (receipt_ref IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS story_tool_operations_task_v1 ON story_tool_operations_v1(project_id,task_id);
CREATE TABLE IF NOT EXISTS story_recovery_fences_v1 (
 fence TEXT PRIMARY KEY, project_id TEXT NOT NULL, task_ids_json TEXT NOT NULL, tasks_json TEXT NOT NULL,
 page_limit INTEGER NOT NULL, expires_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS story_recovery_members_v1 (
 fence TEXT NOT NULL REFERENCES story_recovery_fences_v1(fence) ON DELETE CASCADE,
 ordinal INTEGER NOT NULL, operation_json TEXT NOT NULL, PRIMARY KEY(fence,ordinal)
);
"#;

/// Host-only identity metadata for one root-agent write. Never contains arguments or credentials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryOperationBinding {
    pub project_id: ProjectId,
    pub task_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub request_hash: String,
    pub idempotency_ref: Option<String>,
    pub now: i64,
}

impl SqliteStoryRepository {
    /// Bind an authenticated root call before dispatch. A reused identity must be identical.
    pub fn bind_recovery_operation(
        &mut self,
        binding: RecoveryOperationBinding,
    ) -> Result<(), RepositoryError> {
        self.recovery_operation = None;
        for value in [
            &binding.task_id,
            &binding.thread_id,
            &binding.turn_id,
            &binding.call_id,
        ] {
            if value.is_empty() || value.len() > 160 || value.chars().any(char::is_control) {
                return Err(RepositoryError::TaskBindingInvalid);
            }
        }
        if !matches!(
            binding.tool_name.as_str(),
            "save_brief"
                | "save_property_rules"
                | "save_outline"
                | "save_chapter_plans"
                | "save_draft_checkpoint"
                | "submit_scene_assessment"
                | "submit_revision_outcome"
                | "submit_commitment_extraction"
                | "submit_commitment_candidate"
                | "request_candidate_task"
                | "publish_knowledge_batch"
                | "submit_planning_review_report"
                | "complete_task"
                | "checkpoint_reading"
        ) || !valid_digest(&binding.request_hash)
            || binding
                .idempotency_ref
                .as_deref()
                .is_some_and(|s| !s.strip_prefix("sha256:").is_some_and(valid_digest))
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let task = crate::tasks::load_task(&tx, &binding.task_id)?;
        if task.project_id != binding.project_id
            || task.thread_id != binding.thread_id
            || task
                .turn_id
                .as_deref()
                .is_some_and(|turn| turn != binding.turn_id)
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let existing: Option<(String,String,String,String,Option<String>)> = tx.query_row(
            "SELECT thread_id, turn_id, tool_name, request_hash, idempotency_ref FROM story_tool_operations_v1 WHERE project_id=?1 AND task_id=?2 AND call_id=?3",
            params![binding.project_id.as_str(),binding.task_id,binding.call_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))
        ).optional().map_err(storage_error)?;
        if let Some(existing) = existing {
            if existing
                != (
                    binding.thread_id.clone(),
                    binding.turn_id.clone(),
                    binding.tool_name.clone(),
                    binding.request_hash.clone(),
                    binding.idempotency_ref.clone(),
                )
            {
                return Err(RepositoryError::IdempotencyConflict);
            }
            let rejected: bool = tx
                .query_row(
                    "SELECT state='rejected' FROM story_tool_operations_v1 WHERE operation_id=?1",
                    [operation_id(&binding)],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            if rejected {
                return Err(RepositoryError::InvalidContent(
                    "tool operation was rejected; corrected submission requires a new call identity".into(),
                ));
            }
        } else {
            let count: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM story_tool_operations_v1 WHERE task_id=?1",
                    [&binding.task_id],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            if count >= 10_000 {
                return Err(RepositoryError::InvalidContent(
                    "task operation budget exhausted".into(),
                ));
            }
            tx.execute("INSERT INTO story_tool_operations_v1 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'accepted',NULL,?10,?10)",
                params![operation_id(&binding), binding.project_id.as_str(), binding.task_id, binding.thread_id, binding.turn_id, binding.call_id, binding.tool_name, binding.request_hash, binding.idempotency_ref, binding.now]).map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)?;
        self.recovery_operation = Some(binding);
        Ok(())
    }
}

/// Record a proven pre-write rejection in the same transaction as its validation.
/// Never use this for storage failures or an outcome whose commit is uncertain.
pub(crate) fn reject_operation(
    connection: &Connection,
    binding: Option<&RecoveryOperationBinding>,
    project: &ProjectId,
    task_id: &str,
    tool: &str,
) -> Result<(), RepositoryError> {
    let Some(binding) = binding else {
        return Ok(());
    };
    if binding.project_id != *project || binding.task_id != task_id || binding.tool_name != tool {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let count = connection.execute(
        "UPDATE story_tool_operations_v1 SET state='rejected',updated_at=?1 WHERE operation_id=?2 AND state='accepted'",
        params![binding.now, operation_id(binding)],
    ).map_err(storage_error)?;
    if count != 1 {
        return Err(RepositoryError::CorruptState);
    }
    Ok(())
}

pub(crate) fn commit_operation(
    connection: &Connection,
    binding: Option<&RecoveryOperationBinding>,
    project: &ProjectId,
    task_id: &str,
    tools: &[&str],
    receipt: &str,
) -> Result<(), RepositoryError> {
    let Some(binding) = binding else {
        return Ok(());
    };
    if binding.project_id != *project
        || binding.task_id != task_id
        || !tools.contains(&binding.tool_name.as_str())
        || receipt.is_empty()
        || receipt.len() > 500
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let count = connection.execute("UPDATE story_tool_operations_v1 SET state='committed',receipt_ref=?1,updated_at=?2 WHERE operation_id=?3 AND (state='accepted' OR receipt_ref=?1)",
        params![receipt,binding.now,operation_id(binding)]).map_err(storage_error)?;
    if count != 1 {
        return Err(RepositoryError::CorruptState);
    }
    Ok(())
}

pub(crate) fn commit_candidate(
    connection: &Connection,
    binding: Option<&RecoveryOperationBinding>,
    project: &ProjectId,
    task: Option<&crate::TaskOutputBinding>,
    kind: &str,
    revision: &str,
) -> Result<(), RepositoryError> {
    if binding.is_none() {
        return Ok(());
    }
    let task = task.ok_or(RepositoryError::TaskBindingInvalid)?;
    let tool = match kind {
        "brief" => "save_brief",
        "propertyRules" => "save_property_rules",
        "outline" => "save_outline",
        "plan" => "save_chapter_plans",
        "draft" => "save_draft_checkpoint",
        "sceneAssessment" => "submit_scene_assessment",
        "revisionOutcome" => "submit_revision_outcome",
        "commitmentExtraction" => "submit_commitment_extraction",
        "commitment" => "submit_commitment_candidate",
        _ => return Err(RepositoryError::TaskOutputForbidden),
    };
    let output:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM story_task_outputs_v2 WHERE task_id=?1 AND revision_id=?2)",params![task.task_id,revision],|row|row.get(0)).map_err(storage_error)?;
    if !output {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    commit_operation(
        connection,
        binding,
        project,
        &task.task_id,
        &[tool],
        &format!("runtime:candidate:{revision}"),
    )
}

fn operation_id(binding: &RecoveryOperationBinding) -> String {
    format!(
        "operation-{:x}",
        Sha256::digest(
            json!([
                binding.project_id.as_str(),
                binding.task_id,
                binding.call_id
            ])
            .to_string()
            .as_bytes()
        )
    )
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
