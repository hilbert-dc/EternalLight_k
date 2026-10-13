use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord};

const MAX_REPLY_BYTES: usize = 65_536;

/// A successful engine turn observed by the Author host, never a model tool claim.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordTaskFinalReply {
    pub thread_id: String,
    pub turn_id: String,
    pub intent_revision: i64,
    pub item_id: Option<String>,
    pub text: String,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum FinalReplyState {
    Recorded,
    Truncated,
    Absent,
}

/// Durable reply evidence; neither candidate adoption nor literary goal certification.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskFinalReply {
    pub task_id: String,
    pub project_id: String,
    pub binding: RecordTaskFinalReply,
    pub state: FinalReplyState,
    pub source_digest: String,
    pub receipt_digest: String,
    pub recorded_at: i64,
}



fn validate_source(
    db: &rusqlite::Connection,
    project: &ProjectId,
    task: &TaskRecord,
    binding: &RecordTaskFinalReply,
) -> Result<String, RepositoryError> {
    if task.project_id != *project
        || task.thread_id != binding.thread_id
        || task.turn_id.as_deref() != Some(binding.turn_id.as_str())
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    crate::task_intents::assert_action_intent(db, &task.task_id, binding.intent_revision)?;
    let intents: Vec<crate::TaskIntentRecord> = (1..=binding.intent_revision)
        .map(|revision| crate::task_intents::load_intent(db, &task.task_id, revision))
        .collect::<Result<_, _>>()?;
    Ok(digest(
        &serde_json::json!({"inputManifest":task.input_manifest_json,
        "target":task.target_json,"inputRevision":task.input_revision,"deliveredIntents":intents,
        "operation":task.operation,"model":task.model})
        .to_string(),
    ))
}

fn load(db: &rusqlite::Connection, task: &str) -> Result<Option<TaskFinalReply>, RepositoryError> {
    let encoded: Option<String> = db
        .query_row(
            "SELECT receipt_json FROM story_task_final_replies_v1 WHERE task_id=?1",
            [task],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    encoded
        .map(|text| serde_json::from_str(&text).map_err(|_| RepositoryError::CorruptState))
        .transpose()
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// Public task final reply capability; SQLite retains transaction ownership.
pub trait TaskFinalReplyRepository {
    fn record_task_final_reply(
        &mut self,
        project: &ProjectId,
        task_id: &str,
        binding: RecordTaskFinalReply,
        now: i64,
    ) -> Result<TaskFinalReply, RepositoryError>;
    fn task_final_reply(
        &self,
        project: &ProjectId,
        task_id: &str,
    ) -> Result<Option<TaskFinalReply>, RepositoryError>;
}

impl TaskFinalReplyRepository for SqliteStoryRepository {
    fn record_task_final_reply(
        &mut self,
        project: &ProjectId,
        task_id: &str,
        binding: RecordTaskFinalReply,
        now: i64,
    ) -> Result<TaskFinalReply, RepositoryError> {
        if now < 0
            || binding.text.len() > MAX_REPLY_BYTES
            || binding.turn_id.is_empty()
            || binding.turn_id.len() > 160
            || binding
                .item_id
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > 160)
            || (binding.item_id.is_none() && (!binding.text.is_empty() || binding.truncated))
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let task = crate::tasks::load_task(&tx, task_id)?;
        let source_digest = validate_source(&tx, project, &task, &binding)?;
        let state = if binding.item_id.is_none() {
            FinalReplyState::Absent
        } else if binding.truncated {
            FinalReplyState::Truncated
        } else {
            FinalReplyState::Recorded
        };
        let mut receipt = TaskFinalReply {
            task_id: task_id.into(),
            project_id: project.as_str().into(),
            binding,
            state,
            source_digest,
            receipt_digest: String::new(),
            recorded_at: now,
        };
        receipt.receipt_digest = digest(&serde_json::json!({"taskId":receipt.task_id,
            "projectId":receipt.project_id,"binding":receipt.binding,"sourceDigest":receipt.source_digest}).to_string());
        if let Some(existing) = load(&tx, task_id)? {
            receipt.recorded_at = existing.recorded_at;
            if receipt != existing {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            return Ok(existing);
        }
        if !matches!(
            task.status.as_str(),
            "queued" | "running" | "waiting" | "completed"
        ) {
            return Err(RepositoryError::TaskNotWritable);
        }
        // Current execution authority is checked when creating evidence, not
        // when replaying a historical receipt after later source changes.
        crate::author_message::validate_execution(&tx, &task)?;
        if task.operation == "advisory" {
            crate::task_author_intent::source_intent(
                &tx,
                task_id,
                receipt.binding.intent_revision,
            )?;
        }
        tx.execute(
            "INSERT INTO story_task_final_replies_v1(task_id,receipt_json) VALUES(?1,?2)",
            params![
                task_id,
                serde_json::to_string(&receipt).map_err(|_| RepositoryError::CorruptState)?
            ],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(receipt)
    }

    fn task_final_reply(
        &self,
        project: &ProjectId,
        task_id: &str,
    ) -> Result<Option<TaskFinalReply>, RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let task = crate::tasks::load_task(&tx, task_id)?;
        if task.project_id != *project {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let receipt = load(&tx, task_id)?;
        if let Some(receipt) = &receipt
            && (receipt.task_id != task_id
                || receipt.project_id != project.as_str()
                || validate_source(&tx, project, &task, &receipt.binding)? != receipt.source_digest
                || digest(
                    &serde_json::json!({"taskId":receipt.task_id,"projectId":receipt.project_id,
                    "binding":receipt.binding,"sourceDigest":receipt.source_digest})
                    .to_string(),
                ) != receipt.receipt_digest
                || receipt.state
                    != match (receipt.binding.item_id.is_some(), receipt.binding.truncated) {
                        (false, false) => FinalReplyState::Absent,
                        (true, false) => FinalReplyState::Recorded,
                        (true, true) => FinalReplyState::Truncated,
                        (false, true) => return Err(RepositoryError::CorruptState),
                    }
                || receipt.binding.text.len() > MAX_REPLY_BYTES
                || (receipt.binding.item_id.is_none() && !receipt.binding.text.is_empty()))
        {
            return Err(RepositoryError::CorruptState);
        }
        Ok(receipt)
    }
}
