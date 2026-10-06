//! Ready choices and agent proposals share the same author scope and execution ledger.
use crate::authoring_runs::storage_error;
use crate::{ProjectId, RepositoryError, SqliteStoryRepository, TaskRecord, WorkStep};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkActionRequest {
    pub expected_work_version: i64,
    pub expected_source_digest: String,
    pub action_key: Option<String>,
    pub proposal: Option<crate::WorkActionProposal>,
    pub instruction: String,
    pub idempotency_key: String,
    pub recovery_plan: Option<Value>,
}

impl SqliteStoryRepository {
    pub fn select_work_action(
        &mut self,
        task: &TaskRecord,
        request: &WorkActionRequest,
    ) -> Result<Value, RepositoryError> {
        let outcome = self.select_work_action_transaction(task, request);
        crate::recovery_operations::settle_prewrite_rejection(
            self,
            task,
            "request_work_action",
            &outcome,
        )?;
        outcome
    }
    fn select_work_action_transaction(
        &mut self,
        task: &TaskRecord,
        request: &WorkActionRequest,
    ) -> Result<Value, RepositoryError> {
        if request.instruction.trim().is_empty()
            || request.instruction.chars().count() > 2000
            || request.idempotency_key.is_empty()
            || request.idempotency_key.len() > 160
            || request
                .action_key
                .as_ref()
                .is_some_and(|key| key.is_empty() || key.len() > 80)
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let requested_key = match (&request.action_key, &request.proposal) {
            (Some(key), None) => key.clone(),
            (None, Some(proposal)) => crate::work_action_proposal::key(proposal)?,
            _ => return Err(RepositoryError::TaskBindingInvalid),
        };
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(storage_error)?;
        let bound =
            crate::work_decision::binding(&tx, task)?.ok_or(RepositoryError::TaskBindingInvalid)?;
        let work = bound["workTaskId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        let existing:Option<(String,String,String,String,String)>=tx.query_row("SELECT step_id,action_key,instruction,operation,target_json FROM story_work_decision_actions_v1 WHERE decision_task_id=?1 AND idempotency_key=?2",params![task.task_id,request.idempotency_key],|r|Ok((r.get(/*idx*/0)?,r.get(/*idx*/1)?,r.get(/*idx*/2)?,r.get(/*idx*/3)?,r.get(/*idx*/4)?))).optional().map_err(storage_error)?;
        if let Some((step, key, instruction, operation, encoded_target)) = existing {
            let target: Value =
                serde_json::from_str(&encoded_target).map_err(|_| RepositoryError::CorruptState)?;
            if target.get("recoveryPlan") != request.recovery_plan.as_ref() {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            if key != requested_key || instruction != request.instruction {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            let result = if operation == "finishChapter" {
                finish_response(&bound, &key)
            } else {
                response(self, &tx, &task.project_id, &bound, &step, &key)?
            };
            let reference = if operation == "finishChapter" {
                format!("runtime:chapter-completion:{}", task.task_id)
            } else {
                format!("runtime:work-action:{step}")
            };
            crate::recovery_operations::commit_operation(
                &tx,
                self.recovery_operation.as_ref(),
                &task.project_id,
                &task.task_id,
                &["request_work_action"],
                &reference,
            )?;
            tx.commit().map_err(storage_error)?;
            return Ok(result);
        }
        crate::tasks::validate_active_task(
            &tx,
            task,
            &task.project_id,
            &task.thread_id,
            task.input_revision,
        )?;
        crate::task_input::assert_task_inputs_current(&tx, task)?;
        crate::work_decision::require_cas(
            &tx,
            &task.project_id,
            work,
            &crate::WorkDecisionRequest {
                expected_work_version: request.expected_work_version,
                expected_revision: task.input_revision,
                expected_source_digest: request.expected_source_digest.clone(),
            },
        )?;
        crate::execution_scope_storage::assert_attempt_capacity(&tx, &task.project_id, work)?;
        if crate::work_budget_storage::load(&tx, &task.project_id, work)?.exhausted() {
            return Err(RepositoryError::TaskNotWritable);
        }
        let (_, actions) = crate::work_decision_status::actions(&tx, &task.project_id, work)?;
        let mut action = if let Some(proposal) = &request.proposal {
            crate::work_action_proposal::derive(&tx, &task.project_id, work, proposal, &actions)?
        } else {
            actions
                .into_iter()
                .find(|a| a.key == requested_key)
                .ok_or(RepositoryError::TaskOutputForbidden)?
        };
        crate::work_decision_recovery::validate_plan(
            &tx,
            work,
            &action,
            request.recovery_plan.as_ref(),
        )?;
        if let Some(plan) = &request.recovery_plan {
            action.target["recoveryPlan"] = plan.clone();
        }
        if action.operation == "finishChapter" {
            crate::work_decision_generation::request_finish(&tx, task, &bound, &action)?;
            tx.execute("INSERT INTO story_work_decision_actions_v1(step_id,work_task_id,decision_task_id,action_key,idempotency_key,operation,target_json,instruction,source_task_id,source_revision_id) VALUES(?1,?2,?3,?4,?5,'finishChapter',?6,?7,?8,?9)",params![bound["decisionStepId"].as_str(),work,task.task_id,action.key,request.idempotency_key,action.target.to_string(),request.instruction,action.source_task,action.source_revision]).map_err(storage_error)?;
            crate::recovery_operations::commit_operation(
                &tx,
                self.recovery_operation.as_ref(),
                &task.project_id,
                &task.task_id,
                &["request_work_action"],
                &format!("runtime:chapter-completion:{}", task.task_id),
            )?;
            tx.commit().map_err(storage_error)?;
            return Ok(finish_response(&bound, &action.key));
        }
        if crate::work_decision_generation::completion_selected(&tx, task)? {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if action.operation == "modelKnowledge" {
            let revision = action
                .source_revision
                .as_deref()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            let chapter = action.target["chapterId"]
                .as_str()
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            let batch = format!("modeling-{}", uuid::Uuid::new_v4().simple());
            tx.execute("INSERT OR IGNORE INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,created_at) VALUES(?1,?2,'draft-'||?3,?4,?5,'work-decision-v1','queued',unixepoch())",params![batch,task.project_id.as_str(),chapter,revision,task.input_revision]).map_err(storage_error)?;
            let batch:String=tx.query_row("SELECT batch_id FROM story_modeling_batches_v2 WHERE project_id=?1 AND object_id='draft-'||?2 AND source_revision_id=?3 AND input_project_revision=?4 AND extractor_version='work-decision-v1' AND status IN ('queued','ready')",params![task.project_id.as_str(),chapter,revision,task.input_revision],|r|r.get(/*idx*/0)).map_err(storage_error)?;
            action.target["batchId"] = json!(batch);
        }
        let step = format!("action-{}", uuid::Uuid::new_v4().simple());
        action.target["workDecisionAction"] = json!({"workTaskId":work,"decisionStepId":bound["decisionStepId"],"decisionTaskId":task.task_id,"actionKey":action.key});
        crate::work_decision_generation::bind_action_target(
            &tx,
            &task.project_id,
            work,
            &mut action.target,
            &action.operation,
        )?;
        let revision: i64 = tx
            .query_row(
                "SELECT revision FROM story_workspaces_v2 WHERE project_id=?1",
                [task.project_id.as_str()],
                |r| r.get(/*idx*/ 0),
            )
            .map_err(storage_error)?;
        tx.execute("INSERT INTO story_work_steps_v1(id,work_task_id,ordinal,operation,target_json,input_revision,depends_on_step_id,status) VALUES(?1,?2,(SELECT COALESCE(MAX(ordinal),0)+1 FROM story_work_steps_v1 WHERE work_task_id=?2),?3,?4,?5,?6,'queued')",params![step,work,action.operation,action.target.to_string(),revision,bound["decisionStepId"].as_str()]).map_err(storage_error)?;
        tx.execute("INSERT INTO story_work_decision_actions_v1(step_id,work_task_id,decision_task_id,action_key,idempotency_key,operation,target_json,instruction,source_task_id,source_revision_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![step,work,task.task_id,action.key,request.idempotency_key,action.operation,action.target.to_string(),request.instruction,action.source_task,action.source_revision]).map_err(storage_error)?;
        tx.execute(
            "UPDATE story_work_tasks_v1 SET version=version+1 WHERE id=?1",
            [work],
        )
        .map_err(storage_error)?;
        let result = response(self, &tx, &task.project_id, &bound, &step, &action.key)?;
        crate::recovery_operations::commit_operation(
            &tx,
            self.recovery_operation.as_ref(),
            &task.project_id,
            &task.task_id,
            &["request_work_action"],
            &format!("runtime:work-action:{step}"),
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}
fn response(
    repo: &SqliteStoryRepository,
    db: &rusqlite::Connection,
    project: &ProjectId,
    bound: &Value,
    id: &str,
    key: &str,
) -> Result<Value, RepositoryError> {
    let work = bound["workTaskId"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    let step: WorkStep = crate::work_tasks::load_work_task(repo, db, project, work)?
        .steps
        .into_iter()
        .find(|s| s.id == id)
        .ok_or(RepositoryError::CorruptState)?;
    Ok(
        json!({"workTaskId":work,"threadId":bound["rootThreadId"],"originTaskId":bound["originTaskId"],"decisionStepId":bound["decisionStepId"],"decisionTaskId":bound["decisionTaskId"],"actionKey":key,"step":step,"chapterCompletionRequested":false}),
    )
}

fn finish_response(bound: &Value, key: &str) -> Value {
    json!({"workTaskId":bound["workTaskId"],"threadId":bound["rootThreadId"],"originTaskId":bound["originTaskId"],"decisionStepId":bound["decisionStepId"],"decisionTaskId":bound["decisionTaskId"],"actionKey":key,"step":null,"chapterCompletionRequested":true})
}
