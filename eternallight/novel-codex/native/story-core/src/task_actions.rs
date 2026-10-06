use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskActionRequest {
    pub id: String,
    pub source_task_id: String,
    pub operation: String,
    pub target: Value,
    pub instruction: String,
    pub input_revision: i64,
    pub intent_revision: i64,
    pub status: String,
    pub result_task_id: Option<String>,
    pub error: Option<String>,
}

impl SqliteStoryRepository {
    pub fn record_task_action(
        &mut self,
        project_id: &ProjectId,
        source_task_id: &str,
        idempotency_key: &str,
        action: TaskActionRequest,
    ) -> Result<TaskActionRequest, RepositoryError> {
        let task = crate::tasks::load_task(&self.connection, source_task_id)?;
        let outcome = self.record_task_action_transaction(
            project_id,
            source_task_id,
            idempotency_key,
            action,
        );
        // A rejected proposal wrote neither a candidate nor an executable action.
        // Settle that known result after rollback; storage failures remain unknown.
        crate::recovery_operations::settle_prewrite_rejection(
            self,
            &task,
            "request_candidate_task",
            &outcome,
        )?;
        outcome
    }

    fn record_task_action_transaction(
        &mut self,
        project_id: &ProjectId,
        source_task_id: &str,
        idempotency_key: &str,
        mut action: TaskActionRequest,
    ) -> Result<TaskActionRequest, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if action.source_task_id != source_task_id
            || action.status != "proposed"
            || action.result_task_id.is_some()
            || action.error.is_some()
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let source = tx.query_row(
            "SELECT operation, status, input_revision FROM story_tasks_v2 WHERE project_id = ?1 AND task_id = ?2",
            params![project_id.as_str(), source_task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskNotFound)?;
        if source.0 != "advisory" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let source_task = crate::tasks::load_task(&tx, source_task_id)?;
        let source_target: Value = serde_json::from_str(&source_task.target_json)
            .map_err(|_| RepositoryError::CorruptState)?;
        if source_target["conversationFocus"]["readOnly"].as_bool() == Some(true) {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        let requested_target_json = action.target.to_string();
        let existing = tx.query_row(
            "SELECT action_id, operation, requested_target_json, instruction, input_revision, intent_revision FROM story_task_action_requests_v1 WHERE source_task_id = ?1 AND idempotency_key = ?2",
            params![source_task_id, idempotency_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, i64>(4)?, row.get::<_, i64>(5)?)),
        ).optional().map_err(storage_error)?;
        if let Some((id, operation, target, instruction, revision, intent_revision)) = existing {
            if operation != action.operation
                || target != requested_target_json
                || instruction != action.instruction
                || revision != action.input_revision
                || intent_revision != action.intent_revision
            {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let stored = load_action(&tx, project_id, source_task_id, &id)?;
            crate::recovery_operations::commit_operation(
                &tx,
                self.recovery_operation.as_ref(),
                project_id,
                source_task_id,
                &["request_candidate_task"],
                &format!("runtime:action:{id}"),
            )?;
            tx.commit().map_err(storage_error)?;
            return Ok(stored);
        }
        let current_revision = tx
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
                [project_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(storage_error)?;
        if !matches!(source.1.as_str(), "queued" | "running" | "waiting")
            || source.2 != current_revision
            || source.2 != action.input_revision
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::task_intents::assert_action_intent(&tx, source_task_id, action.intent_revision)?;
        let task = crate::tasks::load_task(&tx, source_task_id)?;
        crate::task_input::assert_task_inputs_current(&tx, &task)?;
        crate::revision_repair::validate_action(&tx, &task, &action.operation, &action.target)?;
        if action.operation == "generateChapter" {
            crate::bound_writer_plan::validate_plan_position(&action.target)
                .map_err(|_| RepositoryError::TaskBindingInvalid)?;
            let chapter_id = action.target["chapterId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            if let Some(plan) = crate::bound_writer_plan::bound_chapter_plan_for_chapter(
                &tx, project_id, chapter_id,
            )? {
                if action.target["chapterNumber"] != plan["chapterNumber"]
                    || action.target["readingChapter"] != plan["chapterNumber"]
                    || action.target["sceneOrder"] != 1
                {
                    return Err(RepositoryError::TaskBindingInvalid);
                }
                for field in ["povCharacterId", "storyTime"] {
                    if action
                        .target
                        .get(field)
                        .is_some_and(|value| value != &plan[field])
                    {
                        return Err(RepositoryError::TaskBindingInvalid);
                    }
                    action.target[field] = plan[field].clone();
                }
            }
        }
        if action.operation == "extractCommitments" {
            crate::commitment_extraction::validate_target(&tx, project_id, &action.target)?;
        }
        if action.operation == "modelKnowledge" {
            crate::modeling_action::validate_target(&tx, project_id, &action.target)?;
        }
        if action.operation == "generateChapter" {
            crate::task_scope::writer_scene_limit(&action.target)?;
            crate::scene_revision::validate_target(&tx, project_id, &action.target)?;
            crate::revision_outcome::desired_outcomes(&action.target)?;
        }
        if crate::work_decision::binding(&tx, &task)?.is_some() {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if action.operation == "modelKnowledge" {
            let writer_pending: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM story_task_action_requests_v1 WHERE source_task_id=?1 AND operation='generateChapter' AND json_extract(target_json,'$.chapterId')=?2 AND status IN ('proposed','queued','running'))", params![source_task_id, action.target["chapterId"].as_str()], |row| row.get(/*idx*/0)).map_err(storage_error)?;
            if writer_pending {
                return Err(RepositoryError::InvalidContent("A pending chapter revision already includes knowledge modeling of its actual saved output; do not bind another action to the old draft".into()));
            }
        }
        let target_json = action.target.to_string();
        if action.operation == "reviseCommitment" {
            crate::commitment_revision::validate_target(&tx, project_id, &action.target)?;
        }
        let parent: String = tx.query_row(
            "SELECT s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id = a.step_id WHERE a.attempt_id = ?1",
            [source_task_id], |row| row.get(0),
        ).map_err(storage_error)?;
        let count = tx
            .query_row(
                "SELECT COUNT(*) FROM story_task_action_requests_v1 WHERE source_task_id = ?1",
                [source_task_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(storage_error)?;
        if count >= 3 {
            return Err(RepositoryError::InvalidContent(
                "one conversation may request at most three candidate tasks".into(),
            ));
        }
        tx.execute(
            "INSERT INTO story_task_action_requests_v1(action_id, project_id, source_task_id, idempotency_key, operation, target_json, requested_target_json, instruction, input_revision, intent_revision) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![action.id, project_id.as_str(), source_task_id, idempotency_key, action.operation, target_json, requested_target_json, action.instruction, action.input_revision, action.intent_revision],
        ).map_err(storage_error)?;
        tx.execute(
            "UPDATE story_work_tasks_v1 SET version = version + 1 WHERE id = ?1",
            [&parent],
        )
        .map_err(storage_error)?;
        crate::recovery_operations::commit_operation(
            &tx,
            self.recovery_operation.as_ref(),
            project_id,
            source_task_id,
            &["request_candidate_task"],
            &format!("runtime:action:{}", action.id),
        )?;
        crate::execution_scope_storage::admit(&tx, project_id, &parent, &action)?;
        let action = load_action(&tx, project_id, source_task_id, &action.id)?;
        tx.commit().map_err(storage_error)?;
        Ok(action)
    }

    /// Only the author route can promote an exact, already validated model proposal
    /// into an executable step. Recheck the frozen source within this transaction.
    pub fn approve_task_action(
        &mut self,
        project_id: &ProjectId,
        source_task_id: &str,
        action_id: &str,
        expected_intent_revision: i64,
        now: i64,
    ) -> Result<TaskActionRequest, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let action = load_action(&tx, project_id, source_task_id, action_id)?;
        if action.intent_revision != expected_intent_revision {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let source = crate::tasks::load_task(&tx, source_task_id)?;
        if crate::work_decision::binding(&tx, &source)?.is_some() {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if crate::task_action_permit::granted(&tx, project_id, &action)? {
            tx.commit().map_err(storage_error)?;
            return Ok(action);
        }
        if action.status != "proposed" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let source = crate::tasks::load_task(&tx, source_task_id)?;
        if source.project_id != *project_id
            || source.operation != "advisory"
            || !matches!(
                source.status.as_str(),
                "queued" | "running" | "waiting" | "completed"
            )
            || source
                .capability_revoked_at
                .is_some_and(|_| source.status != "completed")
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let current_revision: i64 = tx
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id=?1",
                [project_id.as_str()],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if source.input_revision != current_revision || action.input_revision != current_revision {
            return Err(RepositoryError::TaskInputStale);
        }
        crate::task_intents::assert_action_intent(&tx, source_task_id, expected_intent_revision)?;
        crate::task_input::assert_action_inputs_current(&tx, &source)?;
        let (parent, parent_status): (String, String) = tx.query_row(
            "SELECT s.work_task_id,t.status FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 t ON t.id=s.work_task_id WHERE a.attempt_id=?1",
            [source_task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).map_err(storage_error)?;
        if parent_status == "cancelled" {
            return Err(RepositoryError::TaskNotWritable);
        }
        crate::task_action_queue::queue(
            &tx,
            project_id,
            &parent,
            &action,
            crate::task_action_permit::PermissionBasis::ExactAction,
            now,
        )?;
        tx.execute(
            "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1",
            [&parent],
        )
        .map_err(storage_error)?;
        crate::work_tasks::refresh_work_task(&tx, &parent)?;
        let approved = load_action(&tx, project_id, source_task_id, action_id)?;
        tx.commit().map_err(storage_error)?;
        Ok(approved)
    }

    /// Records the author's refusal without creating an executable work step.
    pub fn decline_task_action(
        &mut self,
        project_id: &ProjectId,
        source_task_id: &str,
        action_id: &str,
        expected_intent_revision: i64,
    ) -> Result<TaskActionRequest, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let action = load_action(&tx, project_id, source_task_id, action_id)?;
        if action.intent_revision != expected_intent_revision
            || !matches!(action.status.as_str(), "proposed" | "declined")
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if action.status == "proposed" {
            tx.execute(
                "UPDATE story_task_action_requests_v1 SET status='declined' WHERE action_id=?1 AND status='proposed'",
                [action_id],
            )
            .map_err(storage_error)?;
            tx.execute(
                "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=(SELECT s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE a.attempt_id=?1)",
                [source_task_id],
            )
            .map_err(storage_error)?;
        }
        let declined = load_action(&tx, project_id, source_task_id, action_id)?;
        tx.commit().map_err(storage_error)?;
        Ok(declined)
    }

    pub fn task_actions(
        &self,
        project_id: &ProjectId,
        source_task_id: &str,
    ) -> Result<Vec<TaskActionRequest>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT action_id, source_task_id, operation, target_json, instruction, input_revision, intent_revision, status, result_task_id, error FROM story_task_action_requests_v1 WHERE project_id = ?1 AND source_task_id = ?2 ORDER BY rowid LIMIT 3"
        ).map_err(storage_error)?;
        let rows = statement
            .query_map(params![project_id.as_str(), source_task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                ))
            })
            .map_err(storage_error)?;
        rows.map(|row| {
            let (
                id,
                source_task_id,
                operation,
                target,
                instruction,
                input_revision,
                intent_revision,
                status,
                result_task_id,
                error,
            ) = row.map_err(storage_error)?;
            Ok(TaskActionRequest {
                id,
                source_task_id,
                operation,
                target: serde_json::from_str(&target).map_err(|_| RepositoryError::CorruptState)?,
                instruction,
                input_revision,
                intent_revision,
                status,
                result_task_id,
                error,
            })
        })
        .collect()
    }

    pub fn proposed_task_actions(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<TaskActionRequest>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT a.source_task_id,a.action_id FROM story_task_action_requests_v1 a JOIN story_tasks_v2 t ON t.task_id=a.source_task_id WHERE a.project_id=?1 AND a.status='proposed' AND t.status='completed' ORDER BY a.rowid DESC LIMIT 30",
        ).map_err(storage_error)?;
        let ids = statement
            .query_map([project_id.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        ids.into_iter()
            .map(|(source, action)| load_action(&self.connection, project_id, &source, &action))
            .collect()
    }

    pub fn finish_task_action(
        &mut self,
        project_id: &ProjectId,
        source_task_id: &str,
        action_id: &str,
        result_task_id: Option<&str>,
        error: Option<&str>,
    ) -> Result<TaskActionRequest, RepositoryError> {
        if result_task_id.is_some() == error.is_some()
            || result_task_id.is_some_and(|id| id.is_empty() || id.len() > 160)
            || error.is_some_and(|message| message.trim().is_empty() || message.len() > 400)
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let status = if result_task_id.is_some() {
            "completed"
        } else {
            "failed"
        };
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let stored: (String, Option<String>) = tx.query_row(
            "SELECT status, result_task_id FROM story_task_action_requests_v1 WHERE project_id = ?1 AND source_task_id = ?2 AND action_id = ?3",
            params![project_id.as_str(), source_task_id, action_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
        if let Some(child_id) = result_task_id {
            if stored.1.as_deref() != Some(child_id)
                || !matches!(stored.0.as_str(), "running" | "completed")
            {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let child = crate::tasks::load_task(&tx, child_id)?;
            if child.project_id != *project_id || child.status != "completed" {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let target: Value = serde_json::from_str(&child.target_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            crate::task_action_binding::validate_action_binding(
                &tx,
                project_id,
                source_task_id,
                action_id,
                &child.operation,
                child.input_revision,
                &target,
            )?;
        }
        if stored.0 == status {
            if (result_task_id.is_some() && stored.1.as_deref() == result_task_id)
                || (result_task_id.is_none() && status == "failed")
            {
                let action = load_action(&tx, project_id, source_task_id, action_id)?;
                tx.commit().map_err(storage_error)?;
                return Ok(action);
            }
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if !matches!(stored.0.as_str(), "queued" | "running")
            || (stored.1.is_some()
                && stored.1.as_deref() != result_task_id
                && result_task_id.is_some())
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if status == "failed" && stored.0 == "running" && stored.1.is_some() {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        tx.execute(
            "UPDATE story_task_action_requests_v1 SET status = ?1, result_task_id = COALESCE(?2, result_task_id), error = ?3 WHERE action_id = ?4",
            params![status, result_task_id, error, action_id],
        ).map_err(storage_error)?;
        if status == "failed" {
            let parent: String = tx
                .query_row(
                    "SELECT work_task_id FROM story_work_steps_v1 WHERE id = ?1",
                    [action_id],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            tx.execute("UPDATE story_work_steps_v1 SET status = 'blocked' WHERE id = ?1 AND status = 'queued'", [action_id]).map_err(storage_error)?;
            crate::work_tasks::refresh_work_task(&tx, &parent)?;
        }
        let action = load_action(&tx, project_id, source_task_id, action_id)?;
        tx.commit().map_err(storage_error)?;
        Ok(action)
    }
}

pub(crate) fn load_action(
    db: &Connection,
    project_id: &ProjectId,
    source_task_id: &str,
    action_id: &str,
) -> Result<TaskActionRequest, RepositoryError> {
    let (operation, target, instruction, input_revision, intent_revision, status, result_task_id, error):
        (String, String, String, i64, i64, String, Option<String>, Option<String>) = db.query_row(
        "SELECT operation, target_json, instruction, input_revision, intent_revision, status, result_task_id, error FROM story_task_action_requests_v1 WHERE project_id = ?1 AND source_task_id = ?2 AND action_id = ?3",
        params![project_id.as_str(), source_task_id, action_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    Ok(TaskActionRequest {
        id: action_id.into(),
        source_task_id: source_task_id.into(),
        operation,
        target: serde_json::from_str(&target).map_err(|_| RepositoryError::CorruptState)?,
        instruction,
        input_revision,
        intent_revision,
        status,
        result_task_id,
        error,
    })
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
