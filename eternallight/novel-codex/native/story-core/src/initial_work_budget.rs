use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::authoring_runs::storage_error;
use crate::{
    CreateTaskRequest, ProviderBudget, RepositoryError, SqliteStoryRepository, TaskRecord,
    WorkBudgetAuthorization,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InitialWorkBudget {
    pub authorization_id: String,
    pub author_message_id: String,
    pub expected_base_snapshot_id: String,
    pub expected_source_digest: String,
    pub max_requests: u64,
    pub max_tokens: u64,
    pub deadline_at: i64,
}

impl SqliteStoryRepository {
    /// Creates an author-confirmed root and its first reservation in one transaction.
    /// Exact retries return the same task and never replenish or extend its budget.
    pub fn create_budgeted_root_task(
        &mut self,
        mut request: CreateTaskRequest,
        initial: &InitialWorkBudget,
        now: i64,
    ) -> Result<TaskRecord, RepositoryError> {
        if initial.authorization_id.is_empty()
            || initial.authorization_id == initial.author_message_id
            || initial.authorization_id.len() > 160
            || !initial
                .authorization_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let target: Value = serde_json::from_str(&request.target_json)
            .map_err(|_| RepositoryError::TaskBindingInvalid)?;
        if [
            "runId",
            "resumeStepId",
            "requestedActionId",
            "requestedByTaskId",
            "sourceTaskId",
        ]
        .iter()
        .any(|field| target.get(field).is_some())
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let identity = format!(
            "{:x}",
            Sha256::digest(
                json!([request.project_id.as_str(), initial.authorization_id]).to_string()
            )
        );
        request.task_id = format!("task-{}", &identity[..32]);
        let work_id = format!("work-{}", request.task_id);
        let mut manifest: Value = serde_json::from_str(&request.input_manifest_json)
            .map_err(|_| RepositoryError::TaskBindingInvalid)?;
        if manifest.get("authorMessageId").is_some() {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        manifest["authorMessageId"] = json!(initial.author_message_id);
        request.input_manifest_json = manifest.to_string();
        let fingerprint = format!("{:x}",Sha256::digest(json!({"projectId":request.project_id.as_str(),
            "threadId":request.thread_id,"operation":request.operation,"inputRevision":request.input_revision,
            "model":request.model,"target":target,"manifest":manifest,"capabilityHash":request.capability_hash,
            "capabilityExpiresAt":request.capability_expires_at,"initialBudget":initial}).to_string()));
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let existing: Option<Option<String>> = tx.query_row(
            "SELECT initial_request_digest FROM story_work_budget_authorizations_v1 WHERE work_task_id=?1",
            [&work_id],|row|row.get(0),
        ).optional().map_err(storage_error)?;
        if let Some(existing) = existing {
            if existing.as_deref() != Some(fingerprint.as_str()) {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            crate::work_budget_storage::load(&tx, &request.project_id, &work_id)?;
            let task = crate::tasks::load_task(&tx, &request.task_id)?;
            let manifest: Value = serde_json::from_str(&task.input_manifest_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            crate::author_message::validate_root(&tx, &task, &manifest)?;
            return Ok(task);
        }
        let authorization = WorkBudgetAuthorization {
            expected_version: 0,
            expected_project_revision: request.input_revision,
            expected_source_digest: initial.expected_source_digest.clone(),
            max_requests: initial.max_requests,
            max_tokens: initial.max_tokens,
            deadline_at: initial.deadline_at,
        };
        authorization.validate_limits(now)?;
        let snapshot: String = tx.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",
            [request.project_id.as_str()],|row|row.get(0)).map_err(storage_error)?;
        if snapshot != initial.expected_base_snapshot_id
            || crate::task_input::project_source_digest(&tx, &request.project_id)?
                != initial.expected_source_digest
        {
            return Err(RepositoryError::TaskInputStale);
        }
        let task = crate::tasks::create_task_record(&tx, &request)?;
        crate::author_message::record(&tx, &request, &initial.author_message_id, now)?;
        let budget = ProviderBudget {
            max_requests: initial.max_requests,
            max_tokens: initial.max_tokens,
            used_requests: 0,
            used_tokens: 0,
            unknown_tokens: 0,
            overrun: false,
            charges: Vec::new(),
        };
        tx.execute(
            "INSERT INTO story_work_budgets_v1(work_task_id,ledger_json) VALUES (?1,?2)",
            params![
                work_id,
                serde_json::to_string(&budget).map_err(|_| RepositoryError::CorruptState)?
            ],
        )
        .map_err(storage_error)?;
        tx.execute("INSERT INTO story_work_budget_authorizations_v1(work_task_id,authorization_json,after_attempt_sequence,initial_request_digest,granted_at) VALUES (?1,?2,0,?3,?4)",
            params![work_id,serde_json::to_string(&authorization).map_err(|_|RepositoryError::CorruptState)?,fingerprint,now]).map_err(storage_error)?;
        crate::task_provider_budget::reserve(
            &tx,
            &request.project_id,
            &work_id,
            &request.task_id,
            initial.deadline_at,
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(task)
    }
}
