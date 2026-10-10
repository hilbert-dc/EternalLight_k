use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::Serialize;

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

const MAX_STEERS: i64 = 16;
const MAX_STEER_BYTES: usize = 16_000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskIntentRecord {
    pub task_id: String,
    pub author_message_id: String,
    pub intent_revision: i64,
    pub author_text: String,
    pub status: String,
    pub created_at: i64,
    pub delivered_at: Option<i64>,
}

impl SqliteStoryRepository {
    /// Trusted Host evidence: the directed-input call was rejected before send.
    /// Pause and revoke atomically so the old attempt cannot act on absent input.
    pub fn retain_unsent_task_intent(&mut self, project_id: &ProjectId, task_id: &str, revision: i64, now: i64) -> Result<TaskIntentRecord, RepositoryError> {
        let tx = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(storage_error)?;
        let task = crate::tasks::load_task(&tx, task_id)?;
        if task.project_id != *project_id || task.operation != "advisory" { return Err(RepositoryError::TaskBindingInvalid); }
        let record = load_intent(&tx, task_id, revision)?;
        if record.status == "notSent" { return Ok(record); }
        if record.status != "pending" || intent_head(&tx, task_id)?.0 != revision { return Err(RepositoryError::TaskInputStale); }
        let (work, status): (String, String) = tx.query_row(
            "SELECT w.id,w.status FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND s.current_attempt_id=?1 AND w.project_id=?2 AND w.kind='conversation'",
            params![task_id,project_id.as_str()], |r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
        if !matches!(status.as_str(), "running" | "paused" | "blocked" | "cancelled") { return Err(RepositoryError::TaskNotWritable); }
        if status != "cancelled" {
            if status != "paused" {
                tx.execute("UPDATE story_work_tasks_v1 SET status='paused',version=version+1,control_revision=control_revision+1 WHERE id=?1", [&work]).map_err(storage_error)?;
            }
            crate::work_tasks::pause_active_attempts(&tx, &work, now)?;
            crate::work_pause_barrier::clear(&tx, &work)?;
            crate::work_interludes::invalidate(&tx, &work)?;
        }
        tx.execute("UPDATE story_task_intents_v1 SET status='notSent' WHERE task_id=?1 AND intent_revision=?2 AND status='pending'", params![task_id,revision]).map_err(storage_error)?;
        let retained = load_intent(&tx, task_id, revision)?;
        tx.commit().map_err(storage_error)?;
        Ok(retained)
    }

    pub fn append_task_intent(
        &mut self,
        project_id: &ProjectId,
        task_id: &str,
        expected_intent_revision: i64,
        author_text: &str,
        now: i64,
    ) -> Result<TaskIntentRecord, RepositoryError> {
        if author_text.trim().is_empty() || author_text.len() > MAX_STEER_BYTES {
            return Err(RepositoryError::InvalidContent(
                "invalid author instruction".into(),
            ));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let task = crate::tasks::load_task(&tx, task_id)?;
        if task.project_id != *project_id || task.operation != "advisory" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let (current, pending) = intent_head(&tx, task_id)?;
        if expected_intent_revision < 0 { return Err(RepositoryError::TaskInputStale); }
        if current > expected_intent_revision {
            let receipt = load_intent(&tx, task_id, expected_intent_revision + 1)?;
            return if receipt.author_text == author_text { Ok(receipt) }
                else { Err(RepositoryError::TaskInputStale) };
        }
        if crate::work_interludes::is_attempt(&tx,task_id)? {return Err(RepositoryError::TaskNotWritable);}
        let reply_saved: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM story_task_final_replies_v1 WHERE task_id=?1)",
                [task_id],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if reply_saved {
            return Err(RepositoryError::TaskNotWritable);
        }
        if task.project_id != *project_id
            || task.operation != "advisory"
            || !matches!(task.status.as_str(), "queued" | "running" | "waiting")
            || task.capability_revoked_at.is_some()
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if pending || current != expected_intent_revision || current >= MAX_STEERS {
            return Err(RepositoryError::TaskInputStale);
        }
        let prior = crate::task_author_intent::source_intent(&tx, task_id, current)?;
        if prior.len() + "\n\n作者追加要求：\n".len() + author_text.len()
            > crate::task_author_intent::MAX_AUTHOR_HISTORY_BYTES
        {
            return Err(RepositoryError::InvalidContent(
                "author instruction history exceeds the task budget".into(),
            ));
        }
        let running_actions: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_task_action_requests_v1 WHERE source_task_id=?1 AND status='running')",
            [task_id], |row| row.get(0),
        ).map_err(storage_error)?;
        if running_actions {
            return Err(RepositoryError::TaskInputStale);
        }
        let next = current
            .checked_add(1)
            .ok_or(RepositoryError::CorruptState)?;
        let message = crate::author_intent_message::record(&tx, &task, next, author_text, now)?;
        tx.execute(
            "INSERT INTO story_task_intents_v1(task_id,intent_revision,author_message_id,author_text,status,created_at) VALUES(?1,?2,?3,?4,'pending',?5)",
            params![task_id,next,message,author_text,now],
        )
        .map_err(storage_error)?;
        supersede_queued_actions(&tx, task_id)?;
        tx.execute("UPDATE story_execution_scopes_v1 SET status='superseded',superseded_by_author_message_id=?2 WHERE work_task_id=(SELECT s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE a.attempt_id=?1) AND status='active'",params![task_id,message]).map_err(storage_error)?;
        tx.execute(
            "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=(SELECT s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE a.attempt_id=?1)",
            [task_id],
        ).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(TaskIntentRecord {
            task_id: task_id.into(),
            author_message_id: message,
            intent_revision: next,
            author_text: author_text.into(),
            status: "pending".into(),
            created_at: now,
            delivered_at: None,
        })
    }

    pub fn deliver_task_intent(
        &mut self,
        project_id: &ProjectId,
        task_id: &str,
        intent_revision: i64,
        now: i64,
    ) -> Result<TaskIntentRecord, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let task = crate::tasks::load_task(&tx, task_id)?;
        if task.project_id != *project_id
            || task.operation != "advisory"
            || !matches!(task.status.as_str(), "queued" | "running" | "waiting")
            || task.capability_revoked_at.is_some()
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::author_intent_message::running_work(&tx, &task)?;
        let (current, _) = intent_head(&tx, task_id)?;
        if intent_revision <= 0 || intent_revision != current {
            return Err(RepositoryError::TaskInputStale);
        }
        crate::task_author_intent::source_intent(&tx, task_id, current - 1)?;
        let record = load_intent(&tx, task_id, intent_revision)?;
        if record.status == "delivered" {
            return Ok(record);
        }
        if record.status != "pending" { return Err(RepositoryError::TaskInputStale); }
        tx.execute(
            "UPDATE story_task_intents_v1 SET status='delivered',delivered_at=?1 WHERE task_id=?2 AND intent_revision=?3 AND status='pending'",
            params![now,task_id,intent_revision],
        )
        .map_err(storage_error)?;
        let delivered = load_intent(&tx, task_id, intent_revision)?;
        tx.commit().map_err(storage_error)?;
        Ok(delivered)
    }

    pub fn task_intents(
        &self,
        project_id: &ProjectId,
        task_id: &str,
    ) -> Result<Vec<TaskIntentRecord>, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let task = crate::tasks::load_task(&read, task_id)?;
        if task.project_id != *project_id || task.operation != "advisory" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::author_message::validate_execution(&read, &task)?;
        let mut statement = read.prepare(
            "SELECT intent_revision FROM story_task_intents_v1 WHERE task_id=?1 ORDER BY intent_revision LIMIT 16",
        ).map_err(storage_error)?;
        let records: Vec<TaskIntentRecord> = statement
            .query_map([task_id], |row| row.get::<_, i64>(0))
            .map_err(storage_error)?
            .map(|revision| load_intent(&read, task_id, revision.map_err(storage_error)?))
            .collect::<Result<_, RepositoryError>>()?;
        if records.len() as i64 != intent_head(&read, task_id)?.0
            || records
                .iter()
                .enumerate()
                .any(|(index, record)| record.intent_revision != index as i64 + 1)
        {
            return Err(RepositoryError::CorruptState);
        }
        Ok(records)
    }
}

pub(crate) fn assert_action_intent(
    connection: &rusqlite::Connection,
    task_id: &str,
    expected_intent_revision: i64,
) -> Result<(), RepositoryError> {
    let (current, pending) = intent_head(connection, task_id)?;
    if !pending && expected_intent_revision > current {
        return Err(RepositoryError::InvalidContent(format!(
            "expectedIntentRevision {expected_intent_revision} has not been issued. Read authorIntentState.expectedIntentRevision from read_context (currently {current}); this is separate from the project revision."
        )));
    }
    if pending || current != expected_intent_revision {
        return Err(RepositoryError::TaskInputStale);
    }
    if current > 0 {
        crate::task_author_intent::source_intent(connection, task_id, current)?;
    }
    Ok(())
}

pub(crate) fn supersede_queued_actions(
    db: &rusqlite::Connection,
    task_id: &str,
) -> Result<(), RepositoryError> {
    db.execute(
        "UPDATE story_work_steps_v1 SET status='superseded' WHERE id IN (SELECT action_id FROM story_task_action_requests_v1 WHERE source_task_id=?1 AND status='queued') AND status='queued'",
        [task_id],
    ).map_err(storage_error)?;
    db.execute(
        "UPDATE story_task_action_requests_v1 SET status='superseded',error='Superseded by updated author instructions or a replacement conversation attempt' WHERE source_task_id=?1 AND status IN ('proposed','queued')",
        [task_id],
    ).map_err(storage_error)?;
    Ok(())
}

pub(crate) fn assert_no_pending_intent(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<(), RepositoryError> {
    let (current, pending) = intent_head(connection, task_id)?;
    if pending {
        return Err(RepositoryError::TaskInputStale);
    }
    if current > 0 {
        crate::task_author_intent::source_intent(connection, task_id, current)?;
    }
    Ok(())
}

fn intent_head(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<(i64, bool), RepositoryError> {
    connection.query_row(
        "SELECT intent_revision,status='pending' FROM story_task_intents_v1 WHERE task_id=?1 ORDER BY intent_revision DESC LIMIT 1",
        [task_id],
        |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional().map_err(storage_error).map(|head| head.unwrap_or((0,false)))
}

pub(crate) fn load_intent(
    connection: &rusqlite::Connection,
    task_id: &str,
    revision: i64,
) -> Result<TaskIntentRecord, RepositoryError> {
    let record = connection.query_row(
        "SELECT author_message_id,author_text,status,created_at,delivered_at FROM story_task_intents_v1 WHERE task_id=?1 AND intent_revision=?2",
        params![task_id,revision],
        |row| Ok(TaskIntentRecord {
            task_id: task_id.into(), intent_revision: revision,
            author_message_id: row.get(0)?, author_text: row.get(1)?, status: row.get(2)?,
            created_at: row.get(3)?, delivered_at: row.get(4)?,
        }),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskInputStale)?;
    crate::author_intent_message::validate(connection, &record)?;
    Ok(record)
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
