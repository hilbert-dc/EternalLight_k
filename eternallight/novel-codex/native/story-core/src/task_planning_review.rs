use crate::planning_reviews::{load_run, storage_error};
use crate::{CreatePlanningReview, ProjectId, RepositoryError, SqliteStoryRepository};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskPlanningReviewRequest {
    pub thread_id: String,
    pub expected_revision: i64,
    pub expected_intent_revision: i64,
    pub artifact_kind: String,
    pub revision_id: String,
    pub target_node_id: Option<String>,
    pub roles: Vec<String>,
    pub focus: String,
    pub idempotency_key: String,
}

impl SqliteStoryRepository {
    pub fn request_task_planning_review(
        &mut self,
        project: &ProjectId,
        task_id: &str,
        request: &TaskPlanningReviewRequest,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        if !matches!(request.artifact_kind.as_str(), "outline" | "plan")
            || !crate::planning_reviews::valid_roles(&request.roles)
            || request.focus.chars().count() > 1000
            || request.focus.len() > 4000
            || !crate::author_message::valid_id(&request.idempotency_key)
            || now < 0
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let task = crate::tasks::load_task(&tx, task_id)?;
        if task.project_id != *project
            || task.thread_id != request.thread_id
            || task.operation != "advisory"
            || !matches!(task.status.as_str(), "queued" | "running" | "waiting")
            || task.input_revision != request.expected_revision
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if serde_json::from_str::<Value>(&task.target_json)
            .map_err(|_| RepositoryError::CorruptState)?
            .get("workDecision")
            .is_some()
        {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        let has_resume: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM story_generation_handoffs_v1 WHERE source_task_id=?1 AND json_extract(content_json,'$.resumeRun.id') IS NOT NULL)",
            [task_id], |row| row.get(0)).map_err(storage_error)?;
        if has_resume { return Err(RepositoryError::TaskOutputForbidden); }
        crate::task_input::assert_task_inputs_current(&tx, &task)?;
        crate::task_intents::assert_action_intent(&tx, task_id, request.expected_intent_revision)?;
        let work: String = tx.query_row("SELECT w.id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id
            JOIN story_work_tasks_v1 w ON w.id=s.work_task_id AND w.project_id=?2 AND w.kind='conversation' AND w.status='running'
            WHERE a.attempt_id=?1 AND s.current_attempt_id=?1 AND s.operation='advisory'",params![task_id,project.as_str()],|r|r.get(/*idx*/0))
            .optional().map_err(storage_error)?.ok_or(RepositoryError::TaskBindingInvalid)?;
        let author = crate::planning_review_task_source::load(
            &tx,
            project,
            &work,
            task_id,
            request.expected_intent_revision,
        )?;
        let aligned: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM story_task_input_heads_v2 WHERE task_id=?1 AND object_id=?2
            AND (candidate_revision_id=?3 OR accepted_revision_id=?3 OR working_revision_id=?3))",params![task_id,format!("artifact-{}",request.artifact_kind),request.revision_id],|r|r.get(/*idx*/0)).map_err(storage_error)?;
        if !aligned {
            return Err(RepositoryError::TaskInputStale);
        }
        let hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(request).map_err(|_| RepositoryError::CorruptState)?)
        );
        let existing: Option<(String,String)> = tx.query_row("SELECT run_id,request_hash FROM story_planning_review_runs_v2 WHERE source_task_id=?1 AND request_key=?2",params![task_id,request.idempotency_key],|r|Ok((r.get(/*idx*/0)?,r.get(/*idx*/1)?))).optional().map_err(storage_error)?;
        let run_id = if let Some((id, original_hash)) = existing {
            if original_hash != hash {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            id
        } else {
            crate::model_work_plan::action_ready(&tx, &work)?;
            let count: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM story_planning_review_runs_v2 WHERE source_task_id=?1",
                    [task_id],
                    |r| r.get(/*idx*/ 0),
                )
                .map_err(storage_error)?;
            if count >= 3 {
                return Err(RepositoryError::InvalidContent(
                    "one conversation may request at most three planning reviews".into(),
                ));
            }
            let id = format!(
                "planning-review-{:x}",
                Sha256::digest(
                    json!([task_id, request.idempotency_key])
                        .to_string()
                        .as_bytes()
                )
            );
            crate::planning_review_registration::create(
                &tx,
                &CreatePlanningReview {
                    run_id: &id,
                    project_id: project,
                    artifact_kind: &request.artifact_kind,
                    revision_id: &request.revision_id,
                    target_node_id: request.target_node_id.as_deref(),
                    author_intent: &author.original,
                    author_message_id: &author.id,
                    roles: &request.roles,
                    now,
                },
                crate::planning_review_registration::Origin::Task {
                    work_id: work.clone(),
                    task_id,
                    intent_revision: request.expected_intent_revision,
                    focus: &request.focus,
                    history: &author.history,
                    key: &request.idempotency_key,
                    hash: &hash,
                },
            )?;
            tx.execute(
                "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1 AND project_id=?2",
                params![work, project.as_str()],
            )
            .map_err(storage_error)?;
            crate::work_decision::ensure_author_delivery_pending(&tx, &task)?;
            id
        };
        let run = load_run(&tx, project, &run_id)?;
        crate::recovery_operations::commit_operation(
            &tx,
            self.recovery_operation.as_ref(),
            project,
            task_id,
            &["request_planning_review"],
            &format!("runtime:planning-review:{run_id}"),
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(run)
    }

    pub fn task_planning_reviews(
        &self,
        project: &ProjectId,
        task_id: &str,
    ) -> Result<Vec<Value>, RepositoryError> {
        let read = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let task = crate::tasks::load_task(&read, task_id)?;
        if task.project_id != *project || task.operation != "advisory" {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let mut query=read.prepare("SELECT run_id FROM story_planning_review_runs_v2 WHERE project_id=?1 AND source_task_id=?2 ORDER BY created_at,run_id LIMIT 3").map_err(storage_error)?;
        let ids = query
            .query_map(params![project.as_str(), task_id], |row| {
                row.get::<_, String>(/*idx*/ 0)
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        ids.iter().map(|id| load_run(&read, project, id)).collect()
    }
}
