use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskActionRequest};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionScopeRequest {
    pub action_ids: Vec<String>,
    pub max_actions: u8,
    pub max_task_attempts: u8,
    pub expected_intent_revision: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScopeTarget {
    operation: String,
    target: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionScopePreview {
    pub preview_digest: String,
    pub(crate) request: ExecutionScopeRequest,
    pub(crate) project_id: String,
    pub(crate) work_task_id: String,
    pub(crate) source_task_id: String,
    pub(crate) author_message_id: String,
    pub(crate) author_intent: String,
    pub(crate) input_revision: i64,
    work_version: i64,
    base_snapshot_id: String,
    source_digest: String,
    targets: Vec<ScopeTarget>,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    remaining_requests: Option<u64>,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    remaining_tokens: Option<u64>,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    pub(crate) deadline_at: Option<i64>,
}

impl ExecutionScopePreview {
    pub(crate) fn digest(&self) -> Result<String, RepositoryError> {
        scope_digest(serde_json::to_value(self).map_err(|_| RepositoryError::CorruptState)?)
    }

    pub(crate) fn covers(&self, action: &TaskActionRequest) -> bool {
        self.input_revision == action.input_revision
            && self.targets.iter().any(|template| {
                template.operation == action.operation && template.target == action.target
            })
    }
}



pub(crate) fn scope_digest(mut value: Value) -> Result<String, RepositoryError> {
    value
        .as_object_mut()
        .ok_or(RepositoryError::CorruptState)?
        .remove("previewDigest");
    Ok(format!("{:x}", Sha256::digest(value.to_string())))
}

fn preview(
    db: &Connection,
    project: &ProjectId,
    source_id: &str,
    request: &ExecutionScopeRequest,
    now: i64,
) -> Result<ExecutionScopePreview, RepositoryError> {
    let unique: BTreeSet<&str> = request.action_ids.iter().map(String::as_str).collect();
    if request.action_ids.is_empty()
        || request.action_ids.len() > 3
        || unique.len() != request.action_ids.len()
        || request
            .action_ids
            .iter()
            .any(|id| id.is_empty() || id.len() > 160)
        || !(1..=3).contains(&request.max_actions)
        || request.action_ids.len() > usize::from(request.max_actions)
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if !(2..=16).contains(&request.max_task_attempts) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let source = crate::tasks::load_task(db, source_id)?;
    if source.project_id != *project
        || source.operation != "advisory"
        || !matches!(
            source.status.as_str(),
            "queued" | "running" | "waiting" | "completed"
        )
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    crate::task_intents::assert_action_intent(db, source_id, request.expected_intent_revision)?;
    crate::task_input::assert_task_inputs_current(db, &source)?;
    let author_message_id = crate::author_message::identity(db, source_id)?
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let author_intent =
        crate::task_author_intent::source_intent(db, source_id, request.expected_intent_revision)?;
    let (work_task_id,work_version,status,kind):(String,i64,String,String) = db.query_row(
        "SELECT w.id,w.version,w.status,w.kind FROM story_work_tasks_v1 w JOIN story_work_steps_v1 s ON s.work_task_id=w.id JOIN story_work_attempts_v1 a ON a.step_id=s.id WHERE a.attempt_id=?1 AND w.project_id=?2",
        params![source_id,project.as_str()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).map_err(storage_error)?;
    if kind != "conversation" || !matches!(status.as_str(), "running" | "completed") {
        return Err(RepositoryError::TaskNotWritable);
    }
    let attempts: i64 = db.query_row("SELECT COUNT(*) FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1",[&work_task_id],|row|row.get(0)).map_err(storage_error)?;
    if attempts >= i64::from(request.max_task_attempts) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let budget = crate::work_budget_storage::load(db, project, &work_task_id)?;
    let deadline_at =
        crate::ordinary_work_budget::authorization(db, project, &work_task_id, source_id)?
            .ok_or(RepositoryError::TaskBindingInvalid)?
            .deadline_at;
    if budget.exhausted() || deadline_at.is_some_and(|deadline| deadline <= now) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let mut targets = Vec::new();
    for id in &request.action_ids {
        let action = crate::task_actions::load_action(db, project, source_id, id)?;
        if action.status != "proposed"
            || action.intent_revision != request.expected_intent_revision
            || action.input_revision != source.input_revision
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let template = ScopeTarget {
            operation: action.operation,
            target: action.target,
        };
        if targets.contains(&template) {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        targets.push(template);
    }
    let base_snapshot_id = db.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",[project.as_str()],|row|row.get(0)).map_err(storage_error)?;
    let mut result = ExecutionScopePreview {
        preview_digest: String::new(),
        request: request.clone(),
        project_id: project.to_string(),
        work_task_id,
        source_task_id: source_id.into(),
        author_message_id,
        author_intent,
        input_revision: source.input_revision,
        work_version,
        base_snapshot_id,
        source_digest: crate::task_input::project_source_digest(db, project)?,
        targets,
        remaining_requests: budget.remaining_requests(),
        remaining_tokens: budget.remaining_tokens(),
        deadline_at,
    };
    result.preview_digest = result.digest()?;
    Ok(result)
}

/// Public execution scope capability; SQLite retains transaction ownership.
pub trait ExecutionScopeRepository {
    fn preview_execution_scope(
        &self,
        project: &ProjectId,
        source: &str,
        request: &ExecutionScopeRequest,
        now: i64,
    ) -> Result<ExecutionScopePreview, RepositoryError>;
    fn confirm_execution_scope(
        &mut self,
        project: &ProjectId,
        source: &str,
        request: &ExecutionScopeRequest,
        digest: &str,
        now: i64,
    ) -> Result<Value, RepositoryError>;
    fn execution_scope(
        &self,
        project: &ProjectId,
        source: &str,
    ) -> Result<Option<Value>, RepositoryError>;
}

impl ExecutionScopeRepository for SqliteStoryRepository {
    fn preview_execution_scope(
        &self,
        project: &ProjectId,
        source: &str,
        request: &ExecutionScopeRequest,
        now: i64,
    ) -> Result<ExecutionScopePreview, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        preview(&read, project, source, request, now)
    }

    fn confirm_execution_scope(
        &mut self,
        project: &ProjectId,
        source: &str,
        request: &ExecutionScopeRequest,
        digest: &str,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let id = format!("scope-{digest}");
        if let Some(stored) = crate::execution_scope_storage::load(&tx, project, source, &id)? {
            if stored.scope.request != *request {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            return Ok(json!(stored));
        }
        let scope = preview(&tx, project, source, request, now)?;
        if scope.preview_digest != digest {
            return Err(RepositoryError::TaskInputStale);
        }
        tx.execute("INSERT INTO story_execution_scopes_v1(permit_id,work_task_id,kind,author_message_id,source_task_id,scope_json,preview_digest,confirmed_at,status) VALUES(?1,?2,'advisory',?3,?4,?5,?6,?7,'active')",
            params![id,scope.work_task_id,scope.author_message_id,source,serde_json::to_string(&scope).map_err(|_| RepositoryError::CorruptState)?,digest,now]).map_err(storage_error)?;
        for action_id in &request.action_ids {
            let action = crate::task_actions::load_action(&tx, project, source, action_id)?;
            crate::execution_scope_storage::admit(&tx, project, &scope.work_task_id, &action)?;
        }
        tx.execute(
            "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1",
            [&scope.work_task_id],
        )
        .map_err(storage_error)?;
        let record = crate::execution_scope_storage::load(&tx, project, source, &id)?
            .ok_or(RepositoryError::CorruptState)?;
        tx.commit().map_err(storage_error)?;
        Ok(json!(record))
    }

    fn execution_scope(
        &self,
        project: &ProjectId,
        source: &str,
    ) -> Result<Option<Value>, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let id: Option<String> = read.query_row("SELECT e.permit_id FROM story_execution_scopes_v1 e JOIN story_work_tasks_v1 w ON w.id=e.work_task_id WHERE e.source_task_id=?1 AND w.project_id=?2 ORDER BY e.rowid DESC LIMIT 1",
            params![source,project.as_str()], |row|row.get(0)).optional().map_err(storage_error)?;
        id.map(|id| {
            crate::execution_scope_storage::load(&read, project, source, &id)?
                .map(|record| json!(record))
                .ok_or(RepositoryError::CorruptState)
        })
        .transpose()
    }
}
