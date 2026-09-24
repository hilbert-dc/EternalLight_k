use rusqlite::OptionalExtension;
use rusqlite::params;

use crate::ObjectId;
use crate::ProjectId;
use crate::RepositoryError;
use crate::RevisionId;
use crate::SqliteStoryRepository;
use crate::TaskOutputBinding;

const ACTIVE_STATUSES: [&str; 3] = ["queued", "running", "waiting"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateTaskRequest {
    pub task_id: String,
    pub project_id: ProjectId,
    pub thread_id: String,
    pub operation: String,
    pub input_revision: i64,
    pub model: Option<String>,
    pub target_json: String,
    pub input_manifest_json: String,
    pub capability_hash: String,
    pub capability_expires_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskRecord {
    pub task_id: String,
    pub project_id: ProjectId,
    pub thread_id: String,
    pub operation: String,
    pub task_kind: String,
    pub purpose: String,
    pub required_output_kind: Option<String>,
    pub target_json: String,
    pub input_manifest_json: String,
    pub input_revision: i64,
    pub model: Option<String>,
    pub status: String,
    pub stage: String,
    pub delivery_status: String,
    pub output_refs_json: String,
    pub result_summary: String,
    pub error: Option<String>,
    pub capability_hash: String,
    pub capability_expires_at: i64,
    pub capability_revoked_at: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskUpdate {
    pub status: String,
    pub error: Option<String>,
    pub result_summary: Option<String>,
    pub now: i64,
}

impl SqliteStoryRepository {
    pub fn authorize_task_capability(
        &self,
        project_id: &ProjectId,
        task_id: &str,
        thread_id: &str,
        input_revision: i64,
        capability_hash: &str,
        now: i64,
    ) -> Result<TaskRecord, RepositoryError> {
        let task = load_task(&self.connection, task_id)?;
        validate_active_task(
            &self.connection,
            &task,
            project_id,
            thread_id,
            input_revision,
        )?;
        if task.capability_hash != capability_hash
            || task.capability_revoked_at.is_some()
            || task.capability_expires_at <= now
        {
            return Err(RepositoryError::TaskCapabilityInvalid);
        }
        Ok(task)
    }

    pub fn create_task(
        &mut self,
        request: CreateTaskRequest,
    ) -> Result<TaskRecord, RepositoryError> {
        let transaction = self.connection.transaction().map_err(storage_error)?;
        let task = create_task_record(&transaction, &request)?;
        transaction.commit().map_err(storage_error)?;
        Ok(task)
    }

    pub fn task(
        &self,
        project_id: &ProjectId,
        task_id: &str,
    ) -> Result<Option<TaskRecord>, RepositoryError> {
        let exists = self
            .connection
            .query_row(
                "SELECT 1 FROM story_tasks_v2 WHERE project_id = ?1 AND task_id = ?2",
                params![project_id.as_str(), task_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(storage_error)?
            .is_some();
        exists
            .then(|| load_task(&self.connection, task_id))
            .transpose()
    }

    pub fn tasks(&self, project_id: &ProjectId) -> Result<Vec<TaskRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT task_id FROM story_tasks_v2 WHERE project_id = ?1 ORDER BY rowid DESC LIMIT 30")
            .map_err(storage_error)?;
        statement
            .query_map([project_id.as_str()], |row| row.get::<_, String>(0))
            .map_err(storage_error)?
            .map(|row| load_task(&self.connection, &row.map_err(storage_error)?))
            .collect()
    }

    pub fn update_task(
        &mut self,
        project_id: &ProjectId,
        task_id: &str,
        thread_id: &str,
        update: TaskUpdate,
    ) -> Result<TaskRecord, RepositoryError> {
        if !matches!(
            update.status.as_str(),
            "queued" | "running" | "waiting" | "completed" | "failed" | "cancelled"
        ) {
            return Err(RepositoryError::TaskNotWritable);
        }
        let transaction = self.connection.transaction().map_err(storage_error)?;
        let task = load_task(&transaction, task_id)?;
        if task.project_id != *project_id || task.thread_id != thread_id {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let terminal = matches!(update.status.as_str(), "completed" | "failed" | "cancelled");
        let delivery_status = if update.status == "completed" {
            "ready_for_review"
        } else if terminal && task.output_refs_json != "[]" {
            "incomplete"
        } else {
            "pending"
        };
        transaction
            .execute(
                "UPDATE story_tasks_v2 SET status = ?1, stage = ?1, delivery_status = ?2, error = ?3, result_summary = COALESCE(?4, result_summary), capability_revoked_at = CASE WHEN ?5 THEN ?6 ELSE capability_revoked_at END WHERE task_id = ?7",
                params![update.status, delivery_status, update.error, update.result_summary, terminal, update.now, task_id],
            )
            .map_err(storage_error)?;
        let updated = load_task(&transaction, task_id)?;
        transaction.commit().map_err(storage_error)?;
        Ok(updated)
    }

    pub fn complete_task(
        &mut self,
        project_id: &ProjectId,
        task_id: &str,
        thread_id: &str,
        summary: String,
        now: i64,
    ) -> Result<TaskRecord, RepositoryError> {
        let transaction = self.connection.transaction().map_err(storage_error)?;
        let task = load_task(&transaction, task_id)?;
        validate_active_task(
            &transaction,
            &task,
            project_id,
            thread_id,
            task.input_revision,
        )?;
        if let Some(required) = &task.required_output_kind {
            let output = transaction
                .query_row(
                    "SELECT output_kind, revision_id, completeness FROM story_task_outputs_v2 WHERE task_id = ?1 ORDER BY ordinal DESC LIMIT 1",
                    [task_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?)),
                )
                .optional()
                .map_err(storage_error)?
                .ok_or(RepositoryError::TaskOutputMissing)?;
            if output.0 != *required
                || (required == "draft" && output.2.as_deref() != Some("chapterComplete"))
            {
                return Err(RepositoryError::TaskOutputMissing);
            }
        }
        transaction
            .execute(
                "UPDATE story_tasks_v2 SET status = 'completed', stage = 'ready_for_review', delivery_status = 'ready_for_review', result_summary = ?1, capability_revoked_at = ?2 WHERE task_id = ?3",
                params![summary, now, task_id],
            )
            .map_err(storage_error)?;
        let updated = load_task(&transaction, task_id)?;
        transaction.commit().map_err(storage_error)?;
        Ok(updated)
    }

    pub fn recover_orphaned_tasks(&mut self, now: i64) -> Result<Vec<String>, RepositoryError> {
        let transaction = self.connection.transaction().map_err(storage_error)?;
        let mut statement = transaction
            .prepare(
                "SELECT task_id FROM story_tasks_v2 WHERE status IN ('queued','running','waiting')",
            )
            .map_err(storage_error)?;
        let task_ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(storage_error)?
            .map(|row| row.map_err(storage_error))
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        transaction
            .execute(
                "UPDATE story_tasks_v2 SET status = 'cancelled', stage = 'cancelled', delivery_status = CASE WHEN output_refs_json = '[]' THEN 'pending' ELSE 'incomplete' END, error = '应用重启后已回收孤儿任务', capability_revoked_at = ?1 WHERE status IN ('queued','running','waiting')",
                [now],
            )
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        Ok(task_ids)
    }
}

pub(crate) fn create_task_record(
    connection: &rusqlite::Connection,
    request: &CreateTaskRequest,
) -> Result<TaskRecord, RepositoryError> {
    let (task_kind, purpose, required_output_kind) = operation_contract(&request.operation)?;
    validate_json(&request.target_json)?;
    validate_json(&request.input_manifest_json)?;
    let revision = current_revision(connection, &request.project_id)?;
    if revision != request.input_revision {
        return Err(RepositoryError::StaleProjectRevision {
            expected: request.input_revision,
            actual: revision,
        });
    }
    connection
        .execute(
            "INSERT INTO story_tasks_v2(task_id, project_id, thread_id, operation, task_kind, purpose, required_output_kind, target_json, input_manifest_json, input_revision, model, status, stage, delivery_status, capability_hash, capability_expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'queued', 'preparing', 'pending', ?12, ?13)",
            params![request.task_id, request.project_id.as_str(), request.thread_id, request.operation, task_kind, purpose, required_output_kind, request.target_json, request.input_manifest_json, request.input_revision, request.model, request.capability_hash, request.capability_expires_at],
        )
        .map_err(storage_error)?;
    load_task(connection, &request.task_id)
}

pub(crate) fn validate_task_write(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    binding: &TaskOutputBinding,
    project_revision: i64,
    object_id: &ObjectId,
) -> Result<(), RepositoryError> {
    let task = load_task(connection, &binding.task_id)?;
    validate_active_task(
        connection,
        &task,
        project_id,
        &binding.thread_id,
        project_revision,
    )?;
    let Some(required) = &task.required_output_kind else {
        return Err(RepositoryError::TaskOutputForbidden);
    };
    if binding.output_kind != *required {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    let target: serde_json::Value = serde_json::from_str(&task.target_json)
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    if required == "draft"
        && target
            .get("chapterId")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|chapter_id| object_id.as_str() != format!("draft-{chapter_id}"))
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(())
}

pub(crate) fn record_task_output(
    connection: &rusqlite::Connection,
    binding: &TaskOutputBinding,
    revision_id: &RevisionId,
    object_id: &ObjectId,
) -> Result<(), RepositoryError> {
    let ordinal = connection
        .query_row(
            "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM story_task_outputs_v2 WHERE task_id = ?1",
            [&binding.task_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(storage_error)?;
    connection
        .execute(
            "INSERT INTO story_task_outputs_v2(task_id, ordinal, output_kind, revision_id, completeness) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![binding.task_id, ordinal, binding.output_kind, revision_id.as_str(), binding.completeness],
        )
        .map_err(storage_error)?;
    let reference = serde_json::json!({
        "kind": binding.output_kind,
        "revisionId": revision_id.as_str(),
        "objectId": object_id.as_str()
    });
    connection
        .execute(
            "UPDATE story_tasks_v2 SET status = 'running', stage = 'running', output_refs_json = json_insert(output_refs_json, '$[#]', json(?1)) WHERE task_id = ?2",
            params![reference.to_string(), binding.task_id],
        )
        .map_err(storage_error)?;
    Ok(())
}

fn operation_contract(
    operation: &str,
) -> Result<(&'static str, &'static str, Option<&'static str>), RepositoryError> {
    match operation {
        "advisory" => Ok(("advisory", "planner", None)),
        "generateBrief" => Ok(("brief", "planner", Some("brief"))),
        "generateOutline" => Ok(("outline", "planner", Some("outline"))),
        "generatePlan" => Ok(("plan", "planner", Some("plan"))),
        "generateChapter" => Ok(("draft", "writer", Some("draft"))),
        "reviewPlanningSeat" => Ok(("planningReview", "planner", Some("critique"))),
        _ => Err(RepositoryError::TaskOutputForbidden),
    }
}

fn validate_active_task(
    connection: &rusqlite::Connection,
    task: &TaskRecord,
    project_id: &ProjectId,
    thread_id: &str,
    project_revision: i64,
) -> Result<(), RepositoryError> {
    if task.project_id != *project_id || task.thread_id != thread_id {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if !ACTIVE_STATUSES.contains(&task.status.as_str()) {
        return Err(RepositoryError::TaskNotWritable);
    }
    let current = current_revision(connection, project_id)?;
    if current != project_revision || task.input_revision != current {
        return Err(RepositoryError::StaleProjectRevision {
            expected: task.input_revision,
            actual: current,
        });
    }
    Ok(())
}

pub(crate) fn load_task(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<TaskRecord, RepositoryError> {
    connection
        .query_row(
            "SELECT task_id, project_id, thread_id, operation, task_kind, purpose, required_output_kind, target_json, input_manifest_json, input_revision, model, status, stage, delivery_status, output_refs_json, result_summary, error, capability_hash, capability_expires_at, capability_revoked_at FROM story_tasks_v2 WHERE task_id = ?1",
            [task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?, row.get::<_, Option<String>>(6)?, row.get::<_, String>(7)?, row.get::<_, String>(8)?, row.get::<_, i64>(9)?, row.get::<_, Option<String>>(10)?, row.get::<_, String>(11)?, row.get::<_, String>(12)?, row.get::<_, String>(13)?, row.get::<_, String>(14)?, row.get::<_, String>(15)?, row.get::<_, Option<String>>(16)?, row.get::<_, String>(17)?, row.get::<_, i64>(18)?, row.get::<_, Option<i64>>(19)?)),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::TaskNotFound)
        .and_then(|row| {
            Ok(TaskRecord {
                task_id: row.0,
                project_id: ProjectId::parse(row.1).map_err(|_| RepositoryError::CorruptState)?,
                thread_id: row.2,
                operation: row.3,
                task_kind: row.4,
                purpose: row.5,
                required_output_kind: row.6,
                target_json: row.7,
                input_manifest_json: row.8,
                input_revision: row.9,
                model: row.10,
                status: row.11,
                stage: row.12,
                delivery_status: row.13,
                output_refs_json: row.14,
                result_summary: row.15,
                error: row.16,
                capability_hash: row.17,
                capability_expires_at: row.18,
                capability_revoked_at: row.19,
            })
        })
}

fn current_revision(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
) -> Result<i64, RepositoryError> {
    connection
        .query_row(
            "SELECT revision FROM story_workspaces_v2 WHERE project_id = ?1",
            [project_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::ProjectNotFound)
}

fn validate_json(value: &str) -> Result<(), RepositoryError> {
    serde_json::from_str::<serde_json::Value>(value)
        .map(|_| ())
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
