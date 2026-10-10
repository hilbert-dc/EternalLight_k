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
    /// Validate and anchor an independent report before any candidate write.
    /// A bound, definite validation rejection closes only that tool operation;
    /// it cannot settle provider usage or mark the task or Work complete.
    pub fn prepare_revision_outcome_payload_with_recovery(
        &mut self,
        task: &crate::TaskRecord,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, RepositoryError> {
        let outcome = self.prepare_revision_outcome_payload(task, payload);
        settle_prewrite_rejection(self, task, "submit_revision_outcome", &outcome)?;
        outcome
    }

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
                | "authorize_delegated_writer"
                | "save_delegated_draft"
                | "save_property_rules"
                | "save_outline"
                | "save_chapter_plans"
                | "save_draft_checkpoint"
                | "submit_scene_assessment"
                | "submit_revision_outcome"
                | "submit_commitment_extraction"
                | "submit_commitment_candidate"
                | "request_candidate_task"
                | "request_generation_handoff"
                | "pause_generation_run"
                | "request_work_action"
                | "update_work_plan"
                | "request_planning_review"
                | "publish_knowledge_batch"
                | "submit_planning_review_report"
                | "complete_task"
                | "checkpoint_reading"
                | "record_quality_note"
                | "submit_quality_reading"
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

/// Close a definite domain rejection after the operation transaction has rolled
/// back. A storage error or a committed receipt must retain its actual outcome.
pub(crate) fn settle_prewrite_rejection<T>(
    repo: &mut SqliteStoryRepository,
    task: &crate::TaskRecord,
    tool: &str,
    outcome: &Result<T, RepositoryError>,
) -> Result<(), RepositoryError> {
    if !matches!(
        outcome,
        Err(RepositoryError::InvalidContent(_)
            | RepositoryError::PayloadValidation { .. }
            | RepositoryError::IdempotencyConflict
            | RepositoryError::TaskBindingInvalid
            | RepositoryError::TaskInputStale
            | RepositoryError::TaskNotWritable
            | RepositoryError::TaskOutputMissing
            | RepositoryError::TaskOutputForbidden
            | RepositoryError::WorkTaskStale
            | RepositoryError::WorkUsageUnknown
            | RepositoryError::WorkBudgetExhausted
            | RepositoryError::WorkUsageUnsettled
            | RepositoryError::TaskBudgetAlreadySettled
            | RepositoryError::TaskBudgetExpired
            | RepositoryError::AuthoringRunStale
            | RepositoryError::StaleProjectRevision { .. })
    ) {
        return Ok(());
    }
    let Some(binding) = repo.recovery_operation.as_ref() else {
        return Ok(());
    };
    let tx = rusqlite::Transaction::new_unchecked(
        &repo.connection,
        rusqlite::TransactionBehavior::Immediate,
    )
    .map_err(storage_error)?;
    let state: Option<String> = tx
        .query_row(
            "SELECT state FROM story_tool_operations_v1 WHERE operation_id=?1",
            [operation_id(binding)],
            |row| row.get(/*idx*/ 0),
        )
        .optional()
        .map_err(storage_error)?;
    if state.as_deref() == Some("accepted") {
        reject_operation(&tx, Some(binding), &task.project_id, &task.task_id, tool)?;
    }
    tx.commit().map_err(storage_error)
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
    let tool = candidate_tool(kind)?;
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

pub(crate) fn candidate_tool(kind: &str) -> Result<&'static str, RepositoryError> {
    match kind {
        "brief" => Ok("save_brief"),
        "propertyRules" => Ok("save_property_rules"),
        "outline" => Ok("save_outline"),
        "plan" => Ok("save_chapter_plans"),
        "draft" => Ok("save_draft_checkpoint"),
        "sceneAssessment" => Ok("submit_scene_assessment"),
        "revisionOutcome" => Ok("submit_revision_outcome"),
        "commitmentExtraction" => Ok("submit_commitment_extraction"),
        "commitment" => Ok("submit_commitment_candidate"),
        _ => Err(RepositoryError::TaskOutputForbidden),
    }
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

#[cfg(test)]
mod report_prewrite_rejection_tests {
    use super::*;
    use crate::{BranchId, CreateTaskRequest, ProjectRecord, SnapshotId};

    #[test]
    fn only_definite_payload_rejection_settles_and_unknown_or_success_remains_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = SqliteStoryRepository::open(dir.path().join("prewrite.sqlite3")).unwrap();
        let project = ProjectId::parse("prewrite-proof").unwrap();
        repo.create_project(ProjectRecord {project_id:project.clone(),title:"回执".into(),logline:String::new(),created_at:1,updated_at:1},
            BranchId::parse("main").unwrap(), SnapshotId::parse("initial").unwrap()).unwrap();
        let task = repo.create_task(CreateTaskRequest {task_id:"task".into(),project_id:project.clone(),thread_id:"root".into(),
            operation:"generateBrief".into(),input_revision:0,model:None,target_json:"{}".into(),input_manifest_json:"{}".into(),
            capability_hash:"a".repeat(64),capability_expires_at:9999999999}).unwrap();
        let cases: Vec<(&str, Result<serde_json::Value, RepositoryError>, &str)> = vec![
            ("payload", Err(RepositoryError::PayloadValidation {code:"criterion_set_mismatch",instance_path:"/outcomes".into(),
                details:json!({"expectedCriterionIds":["actual-criterion"]})}), "rejected"),
            ("storage", Err(RepositoryError::Storage("synthetic persistence uncertainty".into())), "accepted"),
            ("corrupt", Err(RepositoryError::CorruptState), "accepted"),
            ("success", Ok(json!({"validated":true})), "accepted"),
        ];
        for (call, outcome, expected) in cases {
            let binding = RecoveryOperationBinding {project_id:project.clone(),task_id:task.task_id.clone(),thread_id:task.thread_id.clone(),
                turn_id:"turn".into(),call_id:call.into(),tool_name:"save_brief".into(),request_hash:"b".repeat(64),idempotency_ref:None,now:10};
            repo.bind_recovery_operation(binding.clone()).unwrap();
            settle_prewrite_rejection(&mut repo, &task, "save_brief", &outcome).unwrap();
            let proof = repo.read_recovery_operation(&binding, None).unwrap().unwrap();
            assert_eq!(proof["state"],expected);
            assert!(proof["receipt_ref"].is_null());
            assert_eq!(repo.task(&project,&task.task_id).unwrap().unwrap(),task);
            assert_eq!(repo.connection.query_row("SELECT COUNT(*) FROM story_revisions_v2",[],|r|r.get::<_,i64>(0)).unwrap(),0);
            assert_eq!(repo.connection.query_row("SELECT COUNT(*) FROM story_task_outputs_v2",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        }
    }
}
