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
    pub turn_id: Option<String>,
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
    pub fn bind_task_turn(
        &mut self,
        project_id: &ProjectId,
        task_id: &str,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<TaskRecord, RepositoryError> {
        if turn_id.is_empty() || turn_id.len() > 160 {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let task = load_task(&tx, task_id)?;
        if task.project_id != *project_id || task.thread_id != thread_id {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if task.turn_id.as_deref() == Some(turn_id) {
            return Ok(task);
        }
        if !(ACTIVE_STATUSES.contains(&task.status.as_str()) || task.status == "completed")
            || task.turn_id.is_some()
        {
            return Err(RepositoryError::TaskNotWritable);
        }
        tx.execute(
            "UPDATE story_tasks_v2 SET turn_id = ?1 WHERE task_id = ?2 AND turn_id IS NULL",
            params![turn_id, task_id],
        )
        .map_err(storage_error)?;
        let updated = load_task(&tx, task_id)?;
        tx.commit().map_err(storage_error)?;
        Ok(updated)
    }

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
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
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
            "running" | "waiting" | "failed" | "cancelled"
        ) {
            return Err(RepositoryError::TaskNotWritable);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let task = load_task(&transaction, task_id)?;
        if task.project_id != *project_id || task.thread_id != thread_id {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        // Repeated delivery is idempotent, but must not overwrite terminal evidence.
        if task.status == update.status {
            return Ok(task);
        }
        if !ACTIVE_STATUSES.contains(&task.status.as_str()) {
            return Err(RepositoryError::TaskNotWritable);
        }
        let terminal = matches!(update.status.as_str(), "failed" | "cancelled");
        let delivery_status = if terminal && task.output_refs_json != "[]" {
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
        if terminal {
            crate::work_tasks::finish_attempt(&transaction, &updated)?;
        }
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
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let task = load_task(&transaction, task_id)?;
        if task.project_id != *project_id || task.thread_id != thread_id {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        // A completed attempt remains completed even if its source later changes.
        if task.status == "completed" {
            crate::recovery_operations::commit_operation(
                &transaction,
                self.recovery_operation.as_ref(),
                project_id,
                task_id,
                &["complete_task"],
                &format!("runtime:task:{task_id}:completed"),
            )?;
            transaction.commit().map_err(storage_error)?;
            return Ok(task);
        }
        validate_active_task(
            &transaction,
            &task,
            project_id,
            thread_id,
            task.input_revision,
        )?;
        crate::task_input::assert_task_inputs_current(&transaction, &task)?;
        if task.operation == "advisory" {
            crate::task_intents::assert_no_pending_intent(&transaction, task_id)?;
        }
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
        if task.operation == "modelKnowledge" {
            let target: serde_json::Value = serde_json::from_str(&task.target_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            let batch_id = target["batchId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            let published = transaction.query_row(
                "SELECT 1 FROM story_modeling_batches_v2 WHERE project_id = ?1 AND batch_id = ?2 AND status = 'ready' AND publisher_task_id = ?3 AND input_project_revision = ?4",
                params![project_id.as_str(), batch_id, task_id, task.input_revision],
                |_| Ok(()),
            ).optional().map_err(storage_error)?.is_some();
            if !published {
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
        crate::work_tasks::finish_attempt(&transaction, &updated)?;
        crate::recovery_operations::commit_operation(
            &transaction,
            self.recovery_operation.as_ref(),
            project_id,
            task_id,
            &["complete_task"],
            &format!("runtime:task:{task_id}:completed"),
        )?;
        transaction.commit().map_err(storage_error)?;
        Ok(updated)
    }

    pub fn recover_orphaned_tasks(&mut self, now: i64) -> Result<Vec<String>, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
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
        for id in &task_ids {
            let before = load_task(&transaction, id)?;
            let recovered = recoverable_output(&transaction, &before)?;
            if recovered {
                transaction.execute(
                    "UPDATE story_tasks_v2 SET status = 'completed', stage = 'ready_for_review', delivery_status = 'ready_for_review', result_summary = '根据已保存产物恢复完成', capability_revoked_at = ?1 WHERE task_id = ?2",
                    params![now, id],
                ).map_err(storage_error)?;
            } else {
                transaction.execute(
                    "UPDATE story_tasks_v2 SET status = 'failed', stage = 'interrupted', delivery_status = CASE WHEN output_refs_json = '[]' THEN 'pending' ELSE 'incomplete' END, error = '应用重启时执行器已中断', capability_revoked_at = ?1 WHERE task_id = ?2",
                    params![now, id],
                ).map_err(storage_error)?;
            }
            let attempt = load_task(&transaction, id)?;
            crate::work_tasks::finish_attempt(&transaction, &attempt)?;
        }
        transaction.commit().map_err(storage_error)?;
        Ok(task_ids)
    }
}

fn recoverable_output(
    connection: &rusqlite::Connection,
    task: &TaskRecord,
) -> Result<bool, RepositoryError> {
    if crate::task_input::assert_task_inputs_current(connection, task).is_err() {
        return Ok(false);
    }
    if task.operation == "modelKnowledge" {
        let target: serde_json::Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        let Some(batch_id) = target.get("batchId").and_then(serde_json::Value::as_str) else {
            return Ok(false);
        };
        return connection.query_row(
            "SELECT 1 FROM story_modeling_batches_v2 WHERE batch_id = ?1 AND project_id = ?2 AND publisher_task_id = ?3 AND input_project_revision = ?4 AND status = 'ready'",
            params![batch_id, task.project_id.as_str(), task.task_id, task.input_revision], |_| Ok(()),
        ).optional().map_err(storage_error).map(|row| row.is_some());
    }
    let Some(required) = task.required_output_kind.as_deref() else {
        return Ok(false);
    };
    let output: Option<(String, String, Option<String>)> = connection.query_row(
        "SELECT output_kind, revision_id, completeness FROM story_task_outputs_v2 WHERE task_id = ?1 ORDER BY ordinal DESC LIMIT 1",
        [&task.task_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional().map_err(storage_error)?;
    let Some((kind, revision_id, completeness)) = output else {
        return Ok(false);
    };
    if kind != required
        || (required == "draft" && completeness.as_deref() != Some("chapterComplete"))
    {
        return Ok(false);
    }
    connection.query_row(
        "SELECT 1 FROM story_object_heads_v2 h JOIN story_revisions_v2 r ON r.project_id = h.project_id AND r.object_id = h.object_id WHERE h.project_id = ?1 AND h.candidate_revision_id = ?2 AND r.revision_id = ?2",
        params![task.project_id.as_str(), revision_id], |_| Ok(()),
    ).optional().map_err(storage_error).map(|row| row.is_some())
}

pub(crate) fn create_task_record(
    connection: &rusqlite::Connection,
    request: &CreateTaskRequest,
) -> Result<TaskRecord, RepositoryError> {
    let (task_kind, purpose, required_output_kind) = operation_contract(&request.operation)?;
    let target: serde_json::Value = serde_json::from_str(&request.target_json)
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    let mut manifest: serde_json::Value = serde_json::from_str(&request.input_manifest_json)
        .map_err(|error| RepositoryError::InvalidContent(error.to_string()))?;
    if !target.is_object() || !manifest.is_object() {
        return Err(RepositoryError::InvalidContent(
            "task target and manifest must be objects".into(),
        ));
    }
    // Reject ambiguous lineage before looking up or freezing any author source.
    crate::task_attempts::validate_origin(&target)?;
    if request.operation == "extractCommitments" {
        crate::commitment_extraction::validate_target(connection, &request.project_id, &target)?;
    }
    if request.operation == "generateChapter" {
        crate::scene_revision::validate_target(connection, &request.project_id, &target)?;
        if !crate::revision_outcome::desired_outcomes(&target)?.is_empty()
            && target.get("requestedActionId").is_none()
            && target.get("resumeStepId").is_none()
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
    }
    if request.operation == "assessRevisionOutcome" {
        crate::revision_outcome::validate_target(connection, &request.project_id, &target)?;
        if !crate::revision_outcome::inputs_current(connection, &request.project_id, &target)? {
            return Err(RepositoryError::TaskInputStale);
        }
    }
    if request.operation == "reviseCommitment" {
        crate::commitment_revision::validate_target(connection, &request.project_id, &target)?;
    }
    if request.operation == "generatePropertyRules" && target["type"] != "artifact" {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let revision = current_revision(connection, &request.project_id)?;
    if revision != request.input_revision {
        return Err(RepositoryError::StaleProjectRevision {
            expected: request.input_revision,
            actual: revision,
        });
    }
    if let Some(output_kind) = required_output_kind {
        let object_id = if output_kind == "draft" {
            let chapter_id = target["chapterId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            format!("draft-{chapter_id}")
        } else if output_kind == "revisionOutcome" {
            let writer = target["writerTaskId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            format!("revision-outcome-{writer}")
        } else if matches!(output_kind, "sceneAssessment" | "commitmentExtraction") {
            let chapter_id = target["chapterId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            let prefix = if output_kind == "sceneAssessment" {
                "scene-assessment"
            } else {
                "commitment-extraction"
            };
            format!("{prefix}-{chapter_id}")
        } else if output_kind == "commitment" {
            let id = target["commitmentId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            format!("commitment-{id}")
        } else if matches!(output_kind, "brief" | "outline" | "plan" | "propertyRules") {
            format!("artifact-{output_kind}")
        } else {
            String::new()
        };
        if !object_id.is_empty() {
            let candidate: Option<String> = connection.query_row(
                "SELECT candidate_revision_id FROM story_object_heads_v2 WHERE project_id = ?1 AND object_id = ?2",
                params![request.project_id.as_str(), object_id],
                |row| row.get(0),
            ).optional().map_err(storage_error)?.flatten();
            manifest["expectedCandidateRevisionId"] = serde_json::json!(candidate);
            manifest["outputObjectId"] = serde_json::json!(object_id);
        }
    }
    crate::task_author_intent::freeze(connection, request, &target, &mut manifest)?;
    crate::revision_repair::freeze(connection, request, &target, &mut manifest)?;
    crate::creative_methods::freeze(connection, request, &target, &mut manifest)?;
    connection
        .execute(
            "INSERT INTO story_tasks_v2(task_id, project_id, thread_id, operation, task_kind, purpose, required_output_kind, target_json, input_manifest_json, input_revision, model, status, stage, delivery_status, capability_hash, capability_expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'queued', 'preparing', 'pending', ?12, ?13)",
            params![request.task_id, request.project_id.as_str(), request.thread_id, request.operation, task_kind, purpose, required_output_kind, request.target_json, manifest.to_string(), request.input_revision, request.model, request.capability_hash, request.capability_expires_at],
        )
        .map_err(storage_error)?;
    crate::task_input::capture_task_inputs(connection, &request.task_id, &request.project_id)?;
    crate::task_attempts::attach_attempt(connection, request)?;
    if request.operation == "reviewPlanningSeat" {
        crate::planning_review_work::bind_claim(connection, request)?;
    }
    load_task(connection, &request.task_id)
}

pub(crate) fn validate_task_write(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    binding: &TaskOutputBinding,
    project_revision: i64,
    object_id: &ObjectId,
    expected_candidate_revision_id: &Option<RevisionId>,
    content_json: &str,
) -> Result<(), RepositoryError> {
    let task = load_task(connection, &binding.task_id)?;
    validate_active_task(
        connection,
        &task,
        project_id,
        &binding.thread_id,
        project_revision,
    )?;
    crate::task_input::assert_task_inputs_current(connection, &task)?;
    crate::task_scope::validate_scoped_write(connection, &task, content_json)?;
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
    let Some((target_object_id, expected)) =
        crate::task_input::expected_task_target(connection, &task)?
    else {
        return Err(RepositoryError::TaskBindingInvalid);
    };
    if target_object_id != *object_id {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if &expected != expected_candidate_revision_id {
        return Err(RepositoryError::TaskInputStale);
    }
    Ok(())
}

pub(crate) fn record_task_output(
    connection: &rusqlite::Connection,
    binding: &TaskOutputBinding,
    revision_id: &RevisionId,
    object_id: &ObjectId,
    working_base_revision_id: Option<&RevisionId>,
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
            "INSERT INTO story_task_outputs_v2(task_id, ordinal, output_kind, revision_id, working_base_revision_id, completeness) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![binding.task_id, ordinal, binding.output_kind, revision_id.as_str(), working_base_revision_id.map(RevisionId::as_str), binding.completeness],
        )
        .map_err(storage_error)?;
    let mut reference = serde_json::json!({
        "kind": binding.output_kind,
        "revisionId": revision_id.as_str(),
        "objectId": object_id.as_str()
    });
    if let Some(source) = working_base_revision_id {
        reference["workingBaseRevisionId"] = serde_json::json!(source.as_str());
    }
    connection
        .execute(
            "UPDATE story_tasks_v2 SET status = 'running', stage = 'running', output_refs_json = json_insert(output_refs_json, '$[#]', json(?1)) WHERE task_id = ?2",
            params![reference.to_string(), binding.task_id],
        )
        .map_err(storage_error)?;
    Ok(())
}

pub(crate) fn operation_contract(
    operation: &str,
) -> Result<(&'static str, &'static str, Option<&'static str>), RepositoryError> {
    match operation {
        "advisory" => Ok(("advisory", "planner", None)),
        "reviseCommitment" => Ok(("commitment", "critic", Some("commitment"))),
        "generateBrief" => Ok(("brief", "planner", Some("brief"))),
        "generatePropertyRules" => Ok(("propertyRules", "planner", Some("propertyRules"))),
        "generateOutline" => Ok(("outline", "planner", Some("outline"))),
        "generatePlan" => Ok(("plan", "planner", Some("plan"))),
        "generateChapter" => Ok(("draft", "writer", Some("draft"))),
        "reviewPlanningSeat" => Ok(("planningReview", "planner", Some("critique"))),
        "modelKnowledge" => Ok(("knowledge", "critic", None)),
        "assessSceneOutcome" => Ok(("sceneAssessment", "critic", Some("sceneAssessment"))),
        "assessRevisionOutcome" => Ok(("revisionOutcome", "critic", Some("revisionOutcome"))),
        "extractCommitments" => Ok((
            "commitmentExtraction",
            "critic",
            Some("commitmentExtraction"),
        )),
        _ => Err(RepositoryError::TaskOutputForbidden),
    }
}

pub(crate) fn validate_active_task(
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
            "SELECT task_id, project_id, thread_id, operation, task_kind, purpose, required_output_kind, target_json, input_manifest_json, input_revision, model, status, stage, delivery_status, output_refs_json, result_summary, error, capability_hash, capability_expires_at, capability_revoked_at, turn_id FROM story_tasks_v2 WHERE task_id = ?1",
            [task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?, row.get::<_, Option<String>>(6)?, row.get::<_, String>(7)?, row.get::<_, String>(8)?, row.get::<_, i64>(9)?, row.get::<_, Option<String>>(10)?, row.get::<_, String>(11)?, row.get::<_, String>(12)?, row.get::<_, String>(13)?, row.get::<_, String>(14)?, row.get::<_, String>(15)?, row.get::<_, Option<String>>(16)?, row.get::<_, String>(17)?, row.get::<_, i64>(18)?, row.get::<_, Option<i64>>(19)?, row.get::<_, Option<String>>(20)?)),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or(RepositoryError::TaskNotFound)
        .and_then(|row| {
            let task = TaskRecord {
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
                turn_id: row.20,
                status: row.11,
                stage: row.12,
                delivery_status: row.13,
                output_refs_json: row.14,
                result_summary: row.15,
                error: row.16,
                capability_hash: row.17,
                capability_expires_at: row.18,
                capability_revoked_at: row.19,
            };
            crate::creative_methods::validate(&task)?;
            Ok(task)
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

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
